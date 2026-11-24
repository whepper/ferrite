// ============================================================================
// Q31 REFERENCE EXTRACT — for independent review of the FPE M2 mirror.
//
// Source: rust/src/aac_enc/psy.rs, working tree at HEAD
// b188415e3c16dbf00706984858c6a568c9b705ff (+ uncommitted M1 seam + M2
// visibility bumps). This file is a VERBATIM extract of the Q31 orchestration
// entry point and every psy.rs helper the FPE mirror calls, with the original
// line ranges noted. It is NOT a standalone compilable unit (imports, shared
// types, and the energy/thresholds/q31/tns kernels live elsewhere); the
// complete compilable file is bundled at source/modified-rust/psy.rs and the
// kernel dependencies at q31-reference/{energy,thresholds,q31,tns,sfb}.rs.
//
// The ONLY M2 change to these functions is the additive `pub(crate)`
// visibility on lowpass_and_zero_check / tns_encode_channel / overflow_rescale
// / advance_thresholds / finish_channel. Bodies are byte-identical to HEAD.
//
// FPE mirror call sites: source/fpe/orchestrate.rs (stage correspondence in
// docs/stage-correspondence.md).
// ============================================================================

// ---- MDCT block-exponent constants (psy.rs:127-131) ----
/// Initial MDCT block exponent for 1024-line transforms (measured constant,
/// `MDCT_E` in every oracle dump).
pub const MDCT_SCALE_LONG: i32 = 12;
/// Initial MDCT block exponent for short transforms (measured constant).
pub const MDCT_SCALE_SHORT: i32 = 9;

// ---- nrg_add_saturate (psy.rs:294-301); used by group_short_channel ----
/// Saturating energy add (`nrgAddSaturate`, `grp_data.cpp:114-116`).
fn nrg_add_saturate(a: i32, b: i32) -> i32 {
    if a >= i32::MAX - b {
        i32::MAX
    } else {
        a + b
    }
}

// ---- lowpass_and_zero_check (psy.rs:411-432); M2 pub(crate) seam ----
/// Lowpass-zero the float spectrum per window; report zeroSpec.
pub(crate) fn lowpass_and_zero_check(
    spectrum: &mut [f64],
    n_windows: usize,
    window_len: usize,
    lowpass: usize,
) -> bool {
    for w in 0..n_windows {
        let base = w * window_len;
        for v in spectrum[base + lowpass..base + window_len].iter_mut() {
            *v = 0.0;
        }
    }
    for w in 0..n_windows {
        for &v in spectrum[w * window_len..w * window_len + lowpass].iter() {
            if v != 0.0 {
                return false;
            }
        }
    }
    true
}

// ---- process_pair_reuse (psy.rs:505-1130); the mirrored Q31 entry ----
/// [`process_pair`] with caller-owned scratch reuse (M5.20): identical
/// arithmetic, ordering, casts, comparisons and thresholds; the workspace
/// only removes the per-call heap allocations of the internal working
/// buffers.
pub fn process_pair_reuse(
    left: &mut PsyChannel,
    right: Option<&mut PsyChannel>,
    lin: ChannelInput<'_>,
    rin: Option<ChannelInput<'_>>,
    tables: &PsyTables,
    ws: &mut PsyScratch,
) -> (ChannelOutput, Option<ChannelOutput>, Option<MsOutput>) {
    let mut right = right;
    let stereo = right.is_some();
    assert_eq!(stereo, rin.is_some(), "channel/state pairing");

    // D-022 robustness preconditions (ordinary assert!, active in release builds):
    // 1. LFE window invariant: every channel marked LFE must have an
    //    effective Long window sequence. Block switching forces LFE to
    //    Long, but pair synchronization can merge an LFE Long with a
    //    partner Short into Short; reject that for both left and right.
    assert!(
        !(lin.is_lfe && lin.seq == WindowSequence::Short),
        "D-022: LFE channel must not enter psy with an effective Short window \
         sequence (block-switch forced Long merged with partner Short)"
    );
    if let Some(rin) = rin.as_ref() {
        assert!(
            !(rin.is_lfe && rin.seq == WindowSequence::Short),
            "D-022: LFE channel must not enter psy with an effective Short window \
             sequence (block-switch forced Long merged with partner Short)"
        );
    }
    // 2. CPE sequence invariant: left and right effective window sequences
    //    must match before MS-energy cross-indexing and regrouping.
    if let Some(rin) = rin.as_ref() {
        assert_eq!(
            lin.seq, rin.seq,
            "D-022: CPE left and right window sequences must match before \
             MS-energy and regrouping"
        );
    }

    // Per-channel static selection.
    let short_l = lin.seq == WindowSequence::Short;
    let short_r = rin
        .as_ref()
        .map(|r| r.seq == WindowSequence::Short)
        .unwrap_or(false);
    // C uses isShortWindow[0] element-globally for nrgShift (psy_main:676);
    // CPE pairs always agree post-sync.
    let short_elem = short_l;
    let conf_l = if short_l {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let conf_r = if short_r {
        &tables.short_cfg
    } else {
        &tables.long_cfg
    };
    let tconf_l = if short_l {
        &tables.tns_short
    } else {
        &tables.tns_long
    };
    let tconf_r = if short_r {
        &tables.tns_short
    } else {
        &tables.tns_long
    };
    let n_win_l = if short_l { 8 } else { 1 };
    // Mono elements have no right channel: zero windows keeps every
    // right-side loop (maxscale, energies, thresholds, MS) a no-op.
    // (Latent panic for mono silence found by survey: n_win_r was 1 with
    // empty per-window vecs.)
    let n_win_r = if stereo {
        if short_r {
            8
        } else {
            1
        }
    } else {
        0
    };
    let wlen_l = if short_l { 128 } else { 1024 };
    let wlen_r = if short_r { 128 } else { 1024 };
    let active_l = if lin.is_lfe {
        conf_l.sfb_active_lfe()
    } else {
        conf_l.sfb_active()
    };
    let lowpass_l = if lin.is_lfe {
        conf_l.lowpass_line_lfe()
    } else {
        conf_l.lowpass_line()
    };
    let (active_r, lowpass_r) = match &rin {
        Some(r) => (
            if r.is_lfe {
                conf_r.sfb_active_lfe()
            } else {
                conf_r.sfb_active()
            },
            if r.is_lfe {
                conf_r.lowpass_line_lfe()
            } else {
                conf_r.lowpass_line()
            },
        ),
        None => (0, 0),
    };
    // Copy Probe flags first (Copy types, no borrow retained), then split
    // the spectrum borrows.
    let (rseq, rlfe, rgroup) = match &rin {
        Some(r) => (r.seq, r.is_lfe, r.grouping),
        None => (WindowSequence::Long, false, (1, [1, 0, 0, 0])),
    };
    let spec_l: &mut [f64] = &mut lin.spectrum[..];
    let mut rin_opt = rin;
    let mut spec_r_opt: Option<&mut [f64]> = rin_opt.as_mut().map(|r| &mut r.spectrum[..]);

    // Lowpass + zeroSpec + initial exponents.
    let zero_l = lowpass_and_zero_check(spec_l, n_win_l, wlen_l, lowpass_l);
    let zero_r = spec_r_opt
        .as_deref_mut()
        .map(|s| lowpass_and_zero_check(s, n_win_r, wlen_r, lowpass_r));
    let mut m_l = if short_l {
        MDCT_SCALE_SHORT
    } else {
        MDCT_SCALE_LONG
    };
    let mut m_r = if short_r {
        MDCT_SCALE_SHORT
    } else {
        MDCT_SCALE_LONG
    };

    // Integer lane: Q31-mapped spectra (round-to-nearest, the single
    // approximation point; all later integer ops are exact).
    // M5.20: written into reused scratch (`clear` + `push` reproduces the
    // former fresh `Vec` exactly: same values, same length, same order).
    energy::map_spectrum_to_q31_into(spec_l, m_l, &mut ws.q_l);

    // --- Rescale stage, joint over the element. ---
    // NOTE: maxScale/energies run unconditionally (zero spectra yield 30s
    // and zeros naturally); the all-zero shortcut below applies per C only
    // when the whole element is silent (mixed elements take the full path,
    // including the Ld offset on zero bands).
    ws.maxsc_l.resize(n_win_l, Vec::new());
    let mut min_spec_shift = i32::MAX;
    for w in 0..n_win_l {
        energy::max_scale_spec_into(
            &ws.q_l[w * wlen_l..(w + 1) * wlen_l],
            conf_l.offsets(),
            active_l,
            &mut ws.maxsc_l[w],
        );
        for &m in ws.maxsc_l[w].iter() {
            min_spec_shift = min_spec_shift.min(m);
        }
    }
    // Right channel contributes to the global minimum (when present).
    ws.q_r.clear();
    ws.maxsc_r.resize(n_win_r, Vec::new());
    if let (Some(spec_r), Some(zero_r)) = (spec_r_opt.as_deref(), zero_r) {
        let _ = zero_r;
        energy::map_spectrum_to_q31_into(spec_r, m_r, &mut ws.q_r);
        for w in 0..n_win_r {
            energy::max_scale_spec_into(
                &ws.q_r[w * wlen_r..(w + 1) * wlen_r],
                conf_r.offsets(),
                active_r,
                &mut ws.maxsc_r[w],
            );
            for &m in ws.maxsc_r[w].iter() {
                min_spec_shift = min_spec_shift.min(m);
            }
        }
    }

    // Energies (stored ints). Log energies derive from the stored values
    // after the re-shift below (see `stored_ld`).
    // M5.20: `e_*` are fully overwritten by the helpers below, but the
    // `eld_*` placeholders (-1.0) are read on the all-zero path (they are
    // the `energy_ld` outputs there), so both are reset to the exact
    // former `vec![...]` contents; inner lengths stay exactly `active_*`.
    ws.e_l.resize(n_win_l, Vec::new());
    ws.eld_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.e_l[w].clear();
        ws.e_l[w].resize(active_l, 0);
        ws.eld_l[w].clear();
        ws.eld_l[w].resize(active_l, -1.0);
    }
    ws.e_r.resize(n_win_r, Vec::new());
    ws.eld_r.resize(n_win_r, Vec::new());
    for w in 0..n_win_r {
        ws.e_r[w].clear();
        ws.e_r[w].resize(active_r, 0);
        ws.eld_r[w].clear();
        ws.eld_r[w].resize(active_r, -1.0);
    }
    let mut max_nrg = 0i32;
    for w in 0..n_win_l {
        energy::check_band_energy_into(
            &ws.q_l[w * wlen_l..(w + 1) * wlen_l],
            &ws.maxsc_l[w],
            conf_l.offsets(),
            active_l,
            &mut ws.e_l[w],
        );
        let m = energy::max_nrg_for_shift(&ws.e_l[w], &ws.maxsc_l[w], min_spec_shift);
        max_nrg = max_nrg.max(m);
    }
    if spec_r_opt.as_deref().is_some() {
        for w in 0..n_win_r {
            energy::check_band_energy_into(
                &ws.q_r[w * wlen_r..(w + 1) * wlen_r],
                &ws.maxsc_r[w],
                conf_r.offsets(),
                active_r,
                &mut ws.e_r[w],
            );
            let m = energy::max_nrg_for_shift(&ws.e_r[w], &ws.maxsc_r[w], min_spec_shift);
            max_nrg = max_nrg.max(m);
        }
    }
    // All-zero shortcut is element-global (psy_main:732): blanks and
    // mdctScale 0 only when every channel is silent; otherwise the full
    // path runs (zero channels get natural zeros plus the Ld offset).
    let zero_elem = zero_l && (!stereo || zero_r == Some(true));
    // `t_*` initial zeros are observable on the all-zero path (they are
    // the `thresholds` outputs there), so reset to exact length+zeros.
    ws.t_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.t_l[w].clear();
        ws.t_l[w].resize(active_l, 0);
    }
    ws.t_r.resize(n_win_r, Vec::new());
    for w in 0..n_win_r {
        ws.t_r[w].clear();
        ws.t_r[w].resize(active_r, 0);
    }
    let final_shift = if zero_elem {
        for w in 0..n_win_l {
            ws.maxsc_l[w].clear();
            ws.maxsc_l[w].resize(active_l, 0);
        }
        for w in 0..n_win_r {
            ws.maxsc_r[w].clear();
            ws.maxsc_r[w].resize(active_r, 0);
        }
        m_l = 0;
        m_r = 0;
        0
    } else {
        let (_, f) = energy::rescale_shifts(max_nrg, min_spec_shift, short_elem, m_l);
        // Apply: Ld from the RAW energies with C's exact guard/shift rules,
        // then stored E re-shift + thresholds init.
        for w in 0..n_win_l {
            energy::rescale_ld_into(&ws.e_l[w], &ws.maxsc_l[w], f, &mut ws.eld_l[w]);
            for b in 0..active_l {
                let mut scale = (ws.maxsc_l[w][b] - 4).max(0);
                scale = ((scale - f) << 1).min(31);
                ws.e_l[w][b] = shift_int(ws.e_l[w][b], -scale);
                ws.t_l[w][b] = init_threshold(ws.e_l[w][b]);
            }
        }
        if stereo {
            for w in 0..n_win_r {
                energy::rescale_ld_into(&ws.e_r[w], &ws.maxsc_r[w], f, &mut ws.eld_r[w]);
                for b in 0..active_r {
                    let mut scale = (ws.maxsc_r[w][b] - 4).max(0);
                    scale = ((scale - f) << 1).min(31);
                    ws.e_r[w][b] = shift_int(ws.e_r[w][b], -scale);
                    ws.t_r[w][b] = init_threshold(ws.e_r[w][b]);
                }
            }
        }
        // Representation shift (C shifts stored ints, tracked by
        // mdctScale; real values unchanged, so the float lane is untouched).
        if f != 0 {
            for v in ws.q_l.iter_mut() {
                *v = shift_int(*v, f);
            }
            for w in 0..n_win_l {
                for m in ws.maxsc_l[w].iter_mut() {
                    *m -= f;
                }
            }
            m_l -= f;
            if spec_r_opt.is_some() {
                for v in ws.q_r.iter_mut() {
                    *v = shift_int(*v, f);
                }
                for w in 0..n_win_r {
                    for m in ws.maxsc_r[w].iter_mut() {
                        *m -= f;
                    }
                }
                m_r -= f;
            }
        }
        f
    };

    // --- TNS block (skipped for LFE elements, mirroring isLFE[0]). ---
    let tns_on = tconf_l.active || tconf_r.active;
    let mut tns_active = [false; 8];
    let mut nrg_scaling = [0i32; 2];
    let mut tns_spec_shift = 0i32;
    if !lin.is_lfe && tns_on {
        // Detect per channel/window on the Q31 stored-domain spectrum
        // (exact integer merge; ratios feed float Schur).
        for w in 0..n_win_l {
            let base = w * wlen_l;
            tns::detect(
                &mut left.tns_state,
                &mut left.tns_info,
                tconf_l,
                &ws.q_l[base..base + wlen_l],
                w,
                short_l,
                conf_l.sfb_count(),
            );
        }
        if stereo {
            let (rstate, rinfo) = right
                .as_deref_mut()
                .map(|r| (&mut r.tns_state, &mut r.tns_info))
                .expect("stereo state");
            let spec_r = spec_r_opt.as_deref().expect("stereo spectrum");
            let _ = spec_r;
            for w in 0..n_win_r {
                let base = w * wlen_r;
                tns::detect(
                    rstate,
                    rinfo,
                    tconf_r,
                    &ws.q_r[base..base + wlen_r],
                    w,
                    short_r,
                    conf_r.sfb_count(),
                );
            }
            // Sync ch1 <- ch0 with ch1's config (psy_main:783-791).
            tns::synchronize_pair(
                rstate,
                rinfo,
                &left.tns_state,
                &left.tns_info,
                tconf_r,
                short_r,
                short_l,
            );
        }
        // Active flags OR over the element (psy_main:793-816).
        let nw = if short_l { 8 } else { 1 };
        for w in 0..nw {
            let l = left.tns_state.active[w];
            let r = right
                .as_deref()
                .map(|r| r.tns_state.active[w])
                .unwrap_or([false, false]);
            // Long windows read subblock 0; short read per-window.
            tns_active[w] = l[0] || l[1] || r[0] || r[1];
        }
        // Pre-encode scaling + encode + energy recompute, per channel.
        // (Two loops mirror C: encode/energy for all channels first so the
        // element-global tnsSpecShift is final before the overflow rescale.)
        // Left channel.
        tns_encode_channel(
            &mut left.tns_state,
            &mut left.tns_info,
            tconf_l,
            conf_l,
            spec_l,
            &mut ws.q_l,
            &mut ws.maxsc_l,
            &mut ws.e_l,
            &mut ws.eld_l,
            &mut ws.t_l,
            &tns_active,
            short_l,
            active_l,
            &mut m_l,
            &mut nrg_scaling[0],
            &mut tns_spec_shift,
            &mut ws.qwin,
            &mut ws.tns_pre,
        );
        // Right channel.
        if stereo {
            let (rstate, rinfo) = right
                .as_deref_mut()
                .map(|r| (&mut r.tns_state, &mut r.tns_info))
                .expect("stereo state");
            let spec_r = spec_r_opt.as_deref_mut().expect("stereo spectrum");
            tns_encode_channel(
                rstate,
                rinfo,
                tconf_r,
                conf_r,
                spec_r,
                &mut ws.q_r,
                &mut ws.maxsc_r,
                &mut ws.e_r,
                &mut ws.eld_r,
                &mut ws.t_r,
                &tns_active,
                short_r,
                active_r,
                &mut m_r,
                &mut nrg_scaling[1],
                &mut tns_spec_shift,
                &mut ws.qwin,
                &mut ws.tns_pre,
            );
        }
    } else {
        // TNS globally inactive: clear runtime decisions (psy_main:915-919).
        // tnsInfo keeps detect-written fields (none ran); info stays as-is.
        left.tns_state.clear_runtime();
        if let Some(r) = right.as_deref_mut() {
            r.tns_state.clear_runtime();
        }
    }
    // Overflow rescale AFTER all channels (element-global tnsSpecShift).
    if tns_spec_shift != 0 {
        overflow_rescale(
            spec_l,
            &mut ws.q_l,
            &mut ws.e_l,
            &mut ws.eld_l,
            &mut ws.t_l,
            &mut m_l,
            lowpass_l,
            active_l,
            tns_spec_shift,
            nrg_scaling[0],
        );
        if stereo {
            let spec_r = spec_r_opt.as_deref_mut().expect("stereo spectrum");
            overflow_rescale(
                spec_r,
                &mut ws.q_r,
                &mut ws.e_r,
                &mut ws.eld_r,
                &mut ws.t_r,
                &mut m_r,
                lowpass_r,
                active_r,
                tns_spec_shift,
                nrg_scaling[1],
            );
        }
    }

    // --- Threshold advance per channel (psy_main:922-1019 minus MS). ---
    // M5.20: `spread_*` placeholders are fully overwritten per band by
    // `advance_thresholds` (copy_from_slice of the energies), reset here
    // to the exact former zeros and exact length.
    ws.spread_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.spread_l[w].clear();
        ws.spread_l[w].resize(active_l, 0);
    }
    advance_thresholds(
        &mut left.preecho,
        &mut ws.t_l,
        &mut ws.spread_l,
        &ws.e_l,
        conf_l,
        active_l,
        m_l,
        short_l,
        lin.seq,
        lin.is_lfe,
    );
    ws.spread_r.clear();
    if stereo {
        ws.spread_r.resize(n_win_r, Vec::new());
        for w in 0..n_win_r {
            ws.spread_r[w].clear();
            ws.spread_r[w].resize(active_r, 0);
        }
        let rstate = right.as_deref_mut().expect("stereo state");
        advance_thresholds(
            &mut rstate.preecho,
            &mut ws.t_r,
            &mut ws.spread_r,
            &ws.e_r,
            conf_r,
            active_r,
            m_r,
            short_r,
            rseq,
            rlfe,
        );
    }

    // --- MS energies (stereo element-level, psy_main:1021-1037). ---
    let ms_out = if stereo {
        // Re-map Q with current exponents (post everything).
        // M5.20: reused scratch (full overwrite by the mapper).
        energy::map_spectrum_to_q31_into(spec_l, m_l, &mut ws.ql_full);
        energy::map_spectrum_to_q31_into(
            spec_r_opt.as_deref().expect("stereo spectrum"),
            m_r,
            &mut ws.qr_full,
        );
        let calc_ld = rseq != WindowSequence::Short;
        let mut mid = Vec::with_capacity(n_win_r);
        let mut side = Vec::with_capacity(n_win_r);
        for w in 0..n_win_r {
            let (mi, si, mil, sil) = energy::calc_ms_energies(
                &ws.ql_full[w * wlen_r..(w + 1) * wlen_r],
                &ws.qr_full[w * wlen_r..(w + 1) * wlen_r],
                &ws.maxsc_l[w],
                &ws.maxsc_r[w],
                conf_r.offsets(),
                active_l,
                calc_ld,
            );
            mid.push(mi);
            side.push(si);
            // C leaves msLd STALE unless calcLd: store on write, read the
            // persistent arrays for output (both short-stale and init-zero
            // mirror C exactly).
            if calc_ld {
                left.ms_ld[w][..active_l].copy_from_slice(&mil);
                let rstate = right.as_deref_mut().expect("stereo state");
                rstate.ms_ld[w][..active_l].copy_from_slice(&sil);
            }
        }
        let rstate = right.as_deref().expect("stereo state");
        let mut mid_ld = Vec::with_capacity(n_win_r);
        let mut side_ld = Vec::with_capacity(n_win_r);
        for w in 0..n_win_r {
            mid_ld.push(left.ms_ld[w][..active_l].to_vec());
            side_ld.push(rstate.ms_ld[w][..active_l].to_vec());
        }
        Some(MsOutput {
            mid,
            side,
            mid_ld,
            side_ld,
        })
    } else {
        None
    };

    // --- Grouping + outputs per channel. ---
    // MS vectors per channel for short-block regrouping (mid ch0/side ch1).
    // Mono MS buffers stay zero (never written in C either).
    ws.ms_zeros_l.resize(n_win_l, Vec::new());
    for w in 0..n_win_l {
        ws.ms_zeros_l[w].clear();
        ws.ms_zeros_l[w].resize(active_l, 0);
    }
    ws.ms_zeros_r.resize(n_win_r, Vec::new());
    for w in 0..n_win_r {
        ws.ms_zeros_r[w].clear();
        ws.ms_zeros_r[w].resize(active_r, 0);
    }
    let (ms_l, ms_r): (&[Vec<i32>], &[Vec<i32>]) = match &ms_out {
        Some(ms) => (&ms.mid, &ms.side),
        None => (&ws.ms_zeros_l, &ws.ms_zeros_r),
    };
    let mut left_out = finish_channel(
        &left.preecho,
        &ws.t_l,
        &ws.e_l,
        &ws.eld_l,
        &ws.spread_l,
        &ws.maxsc_l,
        spec_l,
        conf_l,
        short_l,
        active_l,
        m_l,
        final_shift,
        tns_spec_shift,
        lin.grouping,
        ms_l,
        if short_l {
            &tables.min_snr_short
        } else {
            &tables.min_snr_long
        },
    );
    let mut right_out = if stereo {
        let rstate = right.as_deref_mut().expect("stereo state");
        Some(finish_channel(
            &rstate.preecho,
            &ws.t_r,
            &ws.e_r,
            &ws.eld_r,
            &ws.spread_r,
            &ws.maxsc_r,
            spec_r_opt.as_deref().expect("stereo spectrum"),
            conf_r,
            short_r,
            active_r,
            m_r,
            final_shift,
            tns_spec_shift,
            rgroup,
            ms_r,
            if short_r {
                &tables.min_snr_short
            } else {
                &tables.min_snr_long
            },
        ))
    } else {
        None
    };
    left_out.nrg_scaling = nrg_scaling[0];
    if let Some(ref mut r) = right_out {
        r.nrg_scaling = nrg_scaling[1];
    }
    (left_out, right_out, ms_out)
}

// ---- tns_encode_channel .. grouped_ld_clamped (psy.rs:1132-1551) ----
// tns_encode_channel/overflow_rescale/advance_thresholds/finish_channel
// carry the M2 pub(crate) seam; group_short_channel/grouped_ld/
// grouped_ld_clamped remain private (called only via finish_channel).
/// TNS pre-encode scaling, encode, and energy recompute for one channel
/// (mirrors `psy_main.cpp:818-891`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tns_encode_channel(
    state: &mut TnsChannel,
    info: &mut TnsChannel,
    tconf: &TnsConfig,
    conf: &SfbConfig,
    spectrum: &mut [f64],
    q: &mut Vec<i32>,
    maxsc: &mut [Vec<i32>],
    e_stored: &mut [Vec<i32>],
    eld_true: &mut [Vec<f64>],
    thr: &mut [Vec<i32>],
    tns_active: &[bool; 8],
    short_ch: bool,
    active: usize,
    mdct_scale: &mut i32,
    nrg_scaling_out: &mut i32,
    tns_spec_shift_global: &mut i32,
    // M5.20 scratch: per-window Q31 remap and `band_energy_long` pre-shift
    // accumulations (fully overwritten per window before any read).
    qwin: &mut Vec<i32>,
    pre: &mut Vec<i32>,
) {
    use super::q31::shift_int;
    let n_win = if short_ch { 8 } else { 1 };
    let wlen = if short_ch { 128 } else { 1024 };
    // Pre-encode scaling for long blocks (psy_main:818-839): stored-int
    // halving with mdctScale tracking plus threshold quartering. The float
    // lane carries real values and is untouched (TNS ratios and the linear
    // filter are scale-invariant; post-filter Q remap uses the tracked
    // exponent, matching C's halved-domain values exactly).
    if tns_active[0] && !short_ch {
        for v in q[..conf.lowpass_line()].iter_mut() {
            *v = shift_int(*v, -1);
        }
        for t in thr[0][..active].iter_mut() {
            *t = shift_int(*t, -2);
        }
        *mdct_scale += 1;
    }
    for w in 0..n_win {
        let base = w * wlen;
        tns::encode(
            info,
            state.merged,
            tconf,
            &mut spectrum[base..base + wlen],
            w,
        );
        if tns_active[w] {
            // Recompute headroom on the filtered spectrum (psy_main:860-863).
            // M5.20: `qwin`/`pre` are the reused per-window scratch (the
            // former `qwin` fresh `Vec` is copied into `q` right below, so
            // it never escapes).
            energy::map_spectrum_to_q31_into(&spectrum[base..base + wlen], *mdct_scale, qwin);
            energy::max_scale_spec_into(qwin, conf.offsets(), active, &mut maxsc[w]);
            q[base..base + wlen].copy_from_slice(qwin);
            if short_ch {
                energy::band_energy_short_into(
                    qwin,
                    &maxsc[w],
                    conf.offsets(),
                    active,
                    &mut e_stored[w],
                );
                // Short Ld stays at rescale values (C quirk: Short path
                // takes no Ld array, mirrored).
            } else {
                let k = energy::band_energy_long_into(
                    qwin,
                    &maxsc[w],
                    conf.offsets(),
                    active,
                    pre,
                    &mut e_stored[w],
                    &mut eld_true[w],
                );
                *nrg_scaling_out = k;
                *tns_spec_shift_global = (*tns_spec_shift_global).max(k);
            }
        }
    }
}

/// TNS overflow rescale for one channel (`psy_main.cpp:893-912`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn overflow_rescale(
    spectrum: &mut [f64],
    q: &mut Vec<i32>,
    e_stored: &mut [Vec<i32>],
    eld: &mut [Vec<f64>],
    thr: &mut [Vec<i32>],
    mdct_scale: &mut i32,
    lowpass: usize,
    active: usize,
    tns_spec_shift: i32,
    nrg_scaling: i32,
) {
    use super::q31::shift_int;
    // Representation shift only (stored ints + mdctScale; real values and
    // the float lane are untouched). The `spectrum` slice is kept for
    // signature symmetry with the other stages (grouping reads it later).
    let _ = &mut spectrum[..lowpass];
    for v in q[..lowpass].iter_mut() {
        *v = shift_int(*v, -tns_spec_shift);
    }
    let scale = (tns_spec_shift - nrg_scaling) << 1;
    for b in 0..active {
        e_stored[0][b] = shift_int(e_stored[0][b], -scale);
        thr[0][b] = shift_int(thr[0][b], -(tns_spec_shift << 1));
        // Unconditional Ld correction (mirrors line 903, no zero guard).
        eld[0][b] -= scale as f64 / 64.0;
    }
    *mdct_scale += tns_spec_shift;
}

/// Threshold advance for one channel's windows (`psy_main.cpp:922-1019`
/// minus MS): per-window clip cap, spreading, PCM floor with the
/// accumulating shift, pre-echo with STOP-before/START-after resets, and
/// spread-energy copy plus spreading. `active` is the channel's active
/// band count (LFE-aware; differs from `conf.sfb_active()` for LFE).
#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_thresholds(
    preecho: &mut PreEchoState,
    thr: &mut [Vec<i32>],
    spread: &mut [Vec<i32>],
    energies: &[Vec<i32>],
    conf: &SfbConfig,
    active: usize,
    mdct_scale: i32,
    short_window: bool,
    seq: WindowSequence,
    is_lfe: bool,
) {
    let (mask_hi, mask_lo) = conf.mask_factors_q31();
    let (spr_hi, spr_lo) = conf.mask_factors_spr_q31();
    let clip = clip_cap(conf.clip_energy_q31(), mdct_scale, short_window);
    let mut energy_shift = 2 * mdct_scale;
    for w in 0..thr.len() {
        for t in thr[w][..active].iter_mut() {
            *t = (*t).min(clip);
        }
        spreading_max(active, mask_lo, mask_hi, &mut thr[w]);
        apply_pcm_floor(&mut thr[w], conf.pcm_quant_q31(), &mut energy_shift);
        if !is_lfe {
            if seq == WindowSequence::Stop {
                preecho.reset_for_transition(active);
            }
            preecho.control(&mut thr[w], mdct_scale);
            preecho.set_enabled(true);
            if seq == WindowSequence::Start {
                preecho.reset_for_transition(active);
            }
        }
        spread[w][..active].copy_from_slice(&energies[w][..active]);
        spreading_max(active, spr_lo, spr_hi, &mut spread[w]);
    }
}

/// Group one short channel (`groupShortData`, `grp_data.cpp:118-264`).
/// `ms` holds this channel's regrouped-input MS vectors (mid for ch0, side
/// for ch1; zeros for mono). `min_snr` flows through per group (M4 input
/// until computed). Returns grouped (energies, thresholds, spread, ms,
/// minsnr, offsets, max_sfb, regrouped spectrum).
#[allow(clippy::too_many_arguments)]
fn group_short_channel(
    spectrum: &[f64],
    thr: &[Vec<i32>],
    en: &[Vec<i32>],
    ms: &[Vec<i32>],
    spread: &[Vec<i32>],
    min_snr: &[i32],
    conf: &SfbConfig,
    no_of_groups: usize,
    group_len: &[u8; 4],
) -> (
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    Vec<i32>,
    usize,
    Vec<f64>,
) {
    let offsets: Vec<usize> = conf.offsets().iter().map(|&o| o as usize).collect();
    let active = conf.sfb_active();
    let sfb_cnt = conf.sfb_count();
    // maxSfbPerGroup from top non-zero band across windows.
    let mut highest = 0usize;
    for wnd in 0..8 {
        let mut s = active as i32;
        while s > highest as i32 {
            s -= 1;
            let su = s as usize;
            let mut found = false;
            for line in (offsets[su]..offsets[su + 1]).rev() {
                if spectrum[wnd * 128 + line] != 0.0 {
                    found = true;
                    break;
                }
            }
            if found {
                highest = su;
                break;
            }
        }
    }
    let max_sfb = highest + 1;
    let no_sfb = no_of_groups * sfb_cnt;
    // Grouped offsets (mirrors `grp_data.cpp`: signed `i += sfbCnt - sfb`
    // where `sfb` is the loop-exit value `sfbActive+1`; when
    // `sfbActive == sfbCnt` this steps back one slot and the final
    // `granuleLength` write overwrites the duplicate 1024).
    let mut grouped_off = vec![0i32; no_sfb + 1];
    {
        let mut i: isize = 0;
        let mut offset = 0i32;
        for grp in 0..no_of_groups {
            let gl = group_len[grp] as usize;
            for sfb in 0..active + 1 {
                grouped_off[i as usize] = offset + offsets[sfb] as i32 * gl as i32;
                i += 1;
            }
            // C: `i += sfbCnt - sfb` with `sfb == sfbActive+1` on exit.
            i += sfb_cnt as isize - (active as isize + 1);
            offset += gl as i32 * 128;
        }
        grouped_off[i as usize] = 1024;
    }
    // Generic saturating regroup over int arrays.
    let regroup = |src: &[Vec<i32>]| {
        let mut out = vec![0i32; no_sfb];
        let mut wnd = 0usize;
        let mut i = 0usize;
        for grp in 0..no_of_groups {
            let gl = group_len[grp] as usize;
            for sfb in 0..active {
                let mut acc = src[wnd][sfb];
                for j in 1..gl {
                    acc = nrg_add_saturate(acc, src[wnd + j][sfb]);
                }
                out[i] = acc;
                i += 1;
            }
            i += sfb_cnt - active;
            wnd += gl;
        }
        out
    };
    let grouped_min_snr = {
        let mut out = vec![0i32; no_sfb];
        let mut i = 0usize;
        for grp in 0..no_of_groups {
            for sfb in 0..active {
                out[i] = min_snr[sfb];
                i += 1;
            }
            i += sfb_cnt - active;
            let _ = grp;
        }
        out
    };
    // Regrouped spectrum interleave.
    let mut grouped_spec = vec![0.0f64; 1024];
    {
        let mut wnd = 0usize;
        let mut i = 0usize;
        for grp in 0..no_of_groups {
            let gl = group_len[grp] as usize;
            for sfb in 0..active {
                let width = offsets[sfb + 1] - offsets[sfb];
                for j in 0..gl {
                    let base = (wnd + j) * 128 + offsets[sfb];
                    for k in 0..width {
                        grouped_spec[i] = spectrum[base + k];
                        i += 1;
                    }
                }
            }
            i += gl * (offsets[sfb_cnt] - offsets[active]);
            wnd += gl;
        }
    }
    (
        regroup(en),
        regroup(thr),
        regroup(spread),
        regroup(ms),
        grouped_min_snr,
        grouped_off,
        max_sfb,
        grouped_spec,
    )
}

/// Assemble one channel's output: grouping (short) or long outputs with
/// log-domain vectors. `ms` carries this channel's MS vectors (mid for
/// ch0, side for ch1; zeros for mono) for short-block regrouping.
#[allow(clippy::too_many_arguments)]
pub(crate) fn finish_channel(
    preecho: &PreEchoState,
    thr: &[Vec<i32>],
    en: &[Vec<i32>],
    eld_true: &[Vec<f64>],
    spread: &[Vec<i32>],
    maxsc: &[Vec<i32>],
    spectrum: &[f64],
    conf: &SfbConfig,
    short_ch: bool,
    active: usize,
    mdct_scale: i32,
    final_shift: i32,
    tns_shift: i32,
    grouping: (u8, [u8; 4]),
    ms: &[Vec<i32>],
    min_snr_in: &[i32],
) -> ChannelOutput {
    let mut out = ChannelOutput {
        mdct_scale,
        final_shift,
        tns_shift,
        maxscale: maxsc.to_vec(),
        energies: en.to_vec(),
        energy_ld: eld_true.to_vec(),
        thresholds: thr.to_vec(),
        spread: spread.to_vec(),
        nm1: preecho.prev()[..active].to_vec(),
        m_nm1: preecho.prev_scale(),
        calc_pre_echo: i32::from(preecho.is_enabled()),
        max_sfb_per_group: 0,
        ..Default::default()
    };
    if short_ch {
        let (no_groups, lens) = grouping;
        let (ge, gt, gs, gms, gminsnr, goff, max_sfb, gspec) = group_short_channel(
            spectrum,
            thr,
            en,
            ms,
            spread,
            min_snr_in,
            conf,
            no_groups as usize,
            &lens,
        );
        // Fresh grouped MS Ld refresh (`psy_main.cpp:1078-1084`):
        // exact `CalcLdData` of the grouped sums in the Q31 form the MS
        // decision consumes, never stale history and no float round trip.
        let gms_ld: Vec<i32> = gms.iter().map(|&s| calc_ld_data(s)).collect();
        // M5.20: the Ld vectors are derived before the grouped vectors move
        // into the output (formerly `ge.clone()`/`gt.clone()`/`gs.clone()`
        // allocated three redundant copies per short channel per frame).
        let g_energy_ld = grouped_ld(&ge);
        let g_threshold_ld = grouped_ld_clamped(&gt);
        let grouped = GroupedOutput {
            energies: ge,
            thresholds: gt,
            spread: gs,
            energy_ld: g_energy_ld,
            threshold_ld: g_threshold_ld,
            offsets: goff,
            min_snr: gminsnr,
            spectrum: gspec,
            max_sfb_per_group: max_sfb,
            ms_energies: gms,
            ms_ld: gms_ld,
        };
        out.grouped = Some(grouped);
        out.max_sfb_per_group = max_sfb;
    } else {
        // Long maxSfbPerGroup from top non-zero line (psy_main:1092-1101).
        let offsets: Vec<usize> = conf.offsets().iter().map(|&o| o as usize).collect();
        let mut max_sfb = 0usize;
        for sfb in (0..active).rev() {
            let mut found = false;
            for line in (offsets[sfb]..offsets[sfb + 1]).rev() {
                if spectrum[line] != 0.0 {
                    found = true;
                    break;
                }
            }
            if found {
                max_sfb = sfb + 1;
                break;
            }
        }
        // floor at min(5, active)... C: max(fixMin(5,active), found).
        max_sfb = max_sfb.max(5.min(active));
        out.max_sfb_per_group = max_sfb;
        out.threshold_ld = Some(grouped_ld_clamped(&thr[0]));
    }
    out
}

/// Log-domain of stored ints with the grouped-output floor rule.
/// NOTE: grouped/longout Ld in C comes from `LdDataVector` (fLog2 Taylor);
/// the float `log2/64` used here differs by ~1e-9, covered by tolerance.
fn grouped_ld(stored: &[i32]) -> Vec<f64> {
    stored
        .iter()
        .map(|&v| {
            if v <= 0 {
                super::sfb::ld_data(0.0)
            } else {
                (v as f64 / 2_147_483_648.0).log2() / 64.0
            }
        })
        .collect()
}

/// Threshold Ld with the `-33/64` floor (`psy_main.cpp:1071-1075`).
fn grouped_ld_clamped(stored: &[i32]) -> Vec<f64> {
    grouped_ld(stored)
        .into_iter()
        .map(|v| v.max(super::sfb::THRESHOLD_LD_FLOOR))
        .collect()
}
