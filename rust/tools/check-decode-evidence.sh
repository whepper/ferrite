#!/usr/bin/env bash
# check-decode-evidence.sh — strict evidence checks for the independent
# decoder CI job. Every argument is a captured libtest log.
#
# Usage: check-decode-evidence.sh <crash-log> <m5a-log> <adts-log>
set -euo pipefail

die() {
    printf 'check-decode-evidence: error: %s\n' "$1" >&2
    exit 1
}

[ "$#" -eq 3 ] || die "usage: $0 <crash-log> <m5a-log> <adts-log>"
crash_log=$1
m5a_log=$2
adts_log=$3

for log in "$crash_log" "$m5a_log" "$adts_log"; do
    [ -f "$log" ] || die "missing log file: $log"
    if grep -q '^TIERB_SKIP ' "$log"; then
        marker=$(grep -m1 '^TIERB_SKIP ' "$log")
        die "skip marker in $log: $marker"
    fi
done

grep -qE '^CRASH_RECOVERY_DECODE .*crashed=\[[0-9][0-9, ]*\].*determinism=identical$' "$crash_log" \
    || die "missing CRASH_RECOVERY_DECODE with a nonempty crashed=[...] list and determinism=identical in $crash_log"

m5a_count=$(grep -cE '^M5\.a .*determinism=identical$' "$m5a_log" || true)
[ "$m5a_count" -eq 4 ] \
    || die "expected exactly 4 M5.a deterministic-decode lines in $m5a_log, found $m5a_count"

for stem in m2-click-mono-48k m2-onset-stereo-48k m3-sine-stereo-441k m3-onset-mono-441k; do
    count=$(grep -cE "^M5\\.a ${stem}: .*determinism=identical$" "$m5a_log" || true)
    [ "$count" -eq 1 ] \
        || die "expected exactly one M5.a line for $stem in $m5a_log, found $count"
done

adts_line='test aac_enc::stream_driver::is_integration_tests::intensity_probe_adts_decodes ... ok'
grep -qxF "$adts_line" "$adts_log" \
    || die "missing exact ADTS libtest success line in $adts_log"

printf 'decode evidence OK: crash, m5a, adts\n'
