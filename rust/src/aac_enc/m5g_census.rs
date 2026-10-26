//! Ferrite: part of a Third-Party Modified Version of the Fraunhofer FDK AAC
//! Codec Library for Android. Modifications and Rust implementation copyright
//! © 2025–2026 Jeroen Steenvoorde. Modification notice date: 2026-10-26.
//! Package: ferrite-aac (package license FDK-AAC); see rust/NOTICE for the full license text.
//! M5.g branch-execution census (test-only; read-only observability).
//!
//! Drives the exact 19-stream M5.d inventory (15 M52 + 4 M55) through the
//! existing `m52_write_adts_stream` path, records the per-frame census rows
//! collected from `FrameSnapshot`, asserts the M5.f §7 policy/invariant/
//! accounting criteria, and writes a machine-checkable JSON artifact outside
//! the repo. Encoding decisions are untouched: rows are pure reads, and any
//! skip/note still fails loudly via `m52_emit_or_fail`.
//!
//! M5.i extends the rows with pass-margin operands (all pure reads of final
//! QC state); the M5.g fields and assertions are preserved unchanged.

#[cfg(test)]
mod m5g_tests {
    use super::super::stream_driver::{
        M52_FIXTURES, M55_FIXTURES, m52_write_adts_stream,
    };

    fn out_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("m5g-census")
    }

    #[test]
    fn m5g_branch_census() {
        let dir = out_dir();
        std::fs::create_dir_all(&dir).expect("M5.g: mkdir census dir");
        let mut streams = String::from("{\"streams\":[");
        let mut first_stream = true;
        let mut total_frames = 0usize;
        // M5.i global margin minima across all streams (operands recorded per
        // row above; update here from per-stream minima).
        let mut it_max: i32 = 0;
        let mut grant_min: i64 = i64::MAX;
        let mut grant_argmin = String::new();
        let mut maxdyn_min: i64 = i64::MAX;
        let mut maxdyn_argmin = String::new();
        let mut frame_min: i64 = i64::MAX;
        let mut frame_argmin = String::new();
        let mut cap_min: i64 = i64::MAX;
        let mut cap_argmin = String::new();
        for (short, stem, sr, ch) in M52_FIXTURES
            .iter()
            .chain(M55_FIXTURES.iter())
            .copied()
        {
            let rep = m52_write_adts_stream(short, stem, sr, ch, &dir)
                .unwrap_or_else(|e| panic!("{stem}: encode failed: {e}"));
            assert!(rep.frames > 0, "{stem}: no frames emitted");
            assert_eq!(rep.census.len(), rep.frames, "{stem}: census row count");
            let mut fill_sum: i64 = 0;
            let mut fill_max: i32 = 0;
            let mut fill_nonzero = 0u32;
            // M5.i margin accumulators (operands only; margins derived offline
            // so sign conventions stay auditable in the report, not in code).
            // (Global minima tracked in the outer accumulators below.)
            let mut rows = String::from("[");
            let mut first_row = true;
            for r in &rep.census {
                // M5.f §7 policy: single-pass, no crash recovery, on every
                // emitted frame. (Skips/notes already fail loudly above.)
                assert_eq!(r.npass, 1, "{stem} f={}: npass", r.frame);
                assert_eq!(r.crash, -1, "{stem} f={}: crash", r.frame);
                // Corpus regression: the 19 non-IS fixtures must remain IS-free end to end.
                assert!(
                    !r.is_book_any,
                    "{stem} f={}: nonzero is_book observed",
                    r.frame
                );
                assert!(
                    !r.noise_any_non_pns,
                    "{stem} f={}: non-PNS noise book observed",
                    r.frame
                );
                // Accounting identity (subsumes fill): the byte-exact ADTS
                // length must equal the 7-byte header plus S6-counted bits.
                assert_eq!(
                    r.adts_len as i64 * 8,
                    r.total as i64 + 56,
                    "{stem} f={}: adts/total accounting",
                    r.frame
                );
                fill_sum += r.tot_fill as i64;
                fill_max = fill_max.max(r.tot_fill);
                if r.tot_fill != 0 {
                    fill_nonzero += 1;
                }
                // M5.i margin operands (recorded, not judged: no thresholds
                // are asserted here by design).
                it_max = it_max.max(r.last_iterations);
                let m_grant = r.el_granted as i64 - r.used_dyn as i64;
                if m_grant < grant_min {
                    grant_min = m_grant;
                    grant_argmin = format!("{stem} f={}", r.frame);
                }
                let m_maxdyn = r.max_dyn as i64 - r.used_dyn as i64;
                if m_maxdyn < maxdyn_min {
                    maxdyn_min = m_maxdyn;
                    maxdyn_argmin = format!("{stem} f={}", r.frame);
                }
                let m_frame = r.total_avail as i64 - r.sum_bits as i64;
                if m_frame < frame_min {
                    frame_min = m_frame;
                    frame_argmin = format!("{stem} f={}", r.frame);
                }
                let m_cap = (ch as i64) * 6144 - r.static_used as i64 - r.used_dyn as i64;
                if m_cap < cap_min {
                    cap_min = m_cap;
                    cap_argmin = format!("{stem} f={}", r.frame);
                }
                if !first_row {
                    rows.push(',');
                }
                first_row = false;
                rows.push_str(&format!(
                    "{{\"f\":{},\"seq\":{},\"npass\":{},\"crash\":{},\
                      \"tot_fill\":{},\"total\":{},\"adts_len\":{},\
                      \"is_book\":{},\"noise\":{},\
                      \"iter\":{},\"sum\":{},\"used\":{},\
                      \"grant\":{},\"maxdyn\":{},\"avail\":{},\"static\":{}}}",
                    r.frame,
                    r.seq,
                    r.npass,
                    r.crash,
                    r.tot_fill,
                    r.total,
                    r.adts_len,
                    r.is_book_any,
                    r.noise_any_non_pns,
                    r.last_iterations,
                    r.sum_bits,
                    r.used_dyn,
                    r.el_granted,
                    r.max_dyn,
                    r.total_avail,
                    r.static_used
                ));
            }
            rows.push(']');
            total_frames += rep.frames;
            println!(
                "M5.g {stem}: frames={} bytes={} tot_fill(sum/max/nonzero)=({}/{}/{})",
                rep.frames, rep.bytes, fill_sum, fill_max, fill_nonzero
            );
            // M5.i per-stream margin minima (operands recorded above; margins
            // derived here with documented sign conventions, never asserted).
            let mut s_it = 0i32;
            let mut s_grant = i64::MAX;
            let mut s_maxdyn = i64::MAX;
            let mut s_frame = i64::MAX;
            let mut s_cap = i64::MAX;
            for r in &rep.census {
                s_it = s_it.max(r.last_iterations);
                s_grant = s_grant.min(r.el_granted as i64 - r.used_dyn as i64);
                s_maxdyn = s_maxdyn.min(r.max_dyn as i64 - r.used_dyn as i64);
                s_frame = s_frame.min(r.total_avail as i64 - r.sum_bits as i64);
                s_cap = s_cap.min(
                    (ch as i64) * 6144 - r.static_used as i64 - r.used_dyn as i64,
                );
            }
            println!(
                "M5.i {stem}: max_iter={} min_grant={} min_maxdyn={} min_frame={} min_cap={}",
                s_it, s_grant, s_maxdyn, s_frame, s_cap
            );
            if !first_stream {
                streams.push(',');
            }
            first_stream = false;
            streams.push_str(&format!(
                "{{\"stem\":\"{stem}\",\"sr\":{sr},\"ch\":{ch},\
                  \"frames\":{},\"bytes\":{},\"rows\":{rows}}}",
                rep.frames, rep.bytes
            ));
        }
        streams.push_str("]}");
        std::fs::write(dir.join("census.json"), &streams)
            .expect("M5.g: write census.json");
        println!("M5.g total frames observed: {total_frames}");
        assert_eq!(total_frames, streams.matches("\"f\":").count());
        // M5.i global margin minima (no thresholds asserted by design; the
        // report interprets these against the source-derived boundaries).
        println!(
            "M5.i GLOBAL max_iter={} min_grant={} @ {} min_maxdyn={} @ {} min_frame={} @ {} min_cap={} @ {}",
            it_max,
            grant_min,
            grant_argmin,
            maxdyn_min,
            maxdyn_argmin,
            frame_min,
            frame_argmin,
            cap_min,
            cap_argmin
        );
    }
}
