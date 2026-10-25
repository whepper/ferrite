#!/bin/sh
# tierb-report.sh — Tier-B PASS/SKIP/FAIL classification and summary.
#
# Sourced by run-reference-validation.sh (Stage 6) and by
# test-tierb-report.sh; executes nothing on its own.
#
# Contract: a Tier-B probe that cannot perform its validation prints
# exactly one line of the form
#   TIERB_SKIP <short-test-name> <reason...>
# to stdout (e.g. `TIERB_SKIP intensity_probe_wav_parity
# missing_env=IS_PROBE_WAV`), where <short-test-name> is the trailing
# `::`-component of the libtest path. The probe then returns normally, so
# libtest still reports `... ok`; the marker — visible in the log because
# the driver passes `--nocapture` — is the only signal that nothing was
# validated. A selected test that emits the marker is SKIPPED, never
# passed. (Matching uses the short name derived from the selected path;
# each per-test log segment contains output from exactly one `--exact`
# test, so a marker in a segment belongs to that test.)
#
# Exit codes for the summary: 0 = all selected tests passed,
# 1 = at least one failure, 2 = INCOMPLETE (skips, no failures).

# tierb_classify <test-path> <segment-file> <cargo-exit-status>
# Prints one line: `PASS`, `SKIP <reason>`, or `FAIL`.
tierb_classify() {
    _tc_test=$1
    _tc_seg=$2
    _tc_rc=$3
    _tc_short=${_tc_test##*::}
    _tc_skip=$(grep "^TIERB_SKIP ${_tc_short} " "$_tc_seg" 2>/dev/null | head -n 1 || true)
    if [ -n "$_tc_skip" ]; then
        _tc_reason=$(printf '%s\n' "$_tc_skip" | sed -e 's/^TIERB_SKIP //' -e "s/^${_tc_short} //")
        printf 'SKIP %s\n' "$_tc_reason"
        return 0
    fi
    if [ "$_tc_rc" -eq 0 ] && grep -q "^test ${_tc_test} ... ok$" "$_tc_seg" 2>/dev/null; then
        printf 'PASS\n'
        return 0
    fi
    printf 'FAIL\n'
    return 0
}

# tierb_summarize <passed> <skipped> <failed> <selected>
# Prints the counts line. Returns 0 (clean pass), 1 (failures), or
# 2 (INCOMPLETE: skips but no failures).
tierb_summarize() {
    _ts_passed=$1
    _ts_skipped=$2
    _ts_failed=$3
    _ts_selected=$4
    printf 'Tier-B: %d passed, %d skipped, %d failed of %d selected\n' \
        "$_ts_passed" "$_ts_skipped" "$_ts_failed" "$_ts_selected"
    if [ "$_ts_failed" -ne 0 ]; then
        return 1
    fi
    if [ "$_ts_skipped" -ne 0 ]; then
        return 2
    fi
    return 0
}
