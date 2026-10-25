#!/bin/sh
# test-tierb-report.sh — focused test for tierb-report.sh.
#
# Exercises PASS/SKIP/FAIL classification and the summary exit codes with
# synthetic per-test log segments (no reference checkout, no cargo run).
# Live probe behavior (real SKIP markers, provisioned PASS paths) is
# validated separately with direct `cargo test` invocations; see the F03
# implementation report.
#
# Usage: rust/tools/test-tierb-report.sh
# Exits 0 when all checks pass, 1 otherwise.
set -u

prog=$(basename -- "$0")
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || exit 1
# shellcheck disable=SC1091
. "$SCRIPT_DIR/tierb-report.sh"

TMPDIR_SAFE=${TMPDIR:-/tmp}
TMP=$(mktemp -d "$TMPDIR_SAFE/tierb-report-test.XXXXXX") || exit 1
trap 'rm -rf "$TMP"' EXIT INT TERM

pass=0
fail=0

check() {
    _desc=$1
    _want=$2
    _got=$3
    if [ "$_want" = "$_got" ]; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
        printf '%s: FAIL %s: want [%s] got [%s]\n' "$prog" "$_desc" "$_want" "$_got"
    fi
}

T1="aac_enc::stream_driver::is_integration_tests::intensity_probe_wav_parity"
T2="aac_enc::stream_driver::is_integration_tests::intensity_probe_c_oracle_parity"
S1="intensity_probe_wav_parity"
S2="intensity_probe_c_oracle_parity"

# 1. Clean pass: ok line, exit 0.
cat >"$TMP/pass.log" <<EOF
running 1 test
test $T1 ... ok

test result: ok. 1 passed; 0 failed;
EOF
check "pass-classify" "PASS" "$(tierb_classify "$T1" "$TMP/pass.log" 0)"

# 2. Skip: marker plus libtest ok line (early return still reports ok).
cat >"$TMP/skip.log" <<EOF
running 1 test
TIERB_SKIP $S1 missing_env=IS_PROBE_WAV
test $T1 ... ok

test result: ok. 1 passed; 0 failed;
EOF
check "skip-classify" "SKIP missing_env=IS_PROBE_WAV" "$(tierb_classify "$T1" "$TMP/skip.log" 0)"

# 3. Skip with multi-var reason.
cat >"$TMP/skip2.log" <<EOF
running 1 test
TIERB_SKIP $S2 missing_env=IS_PROBE_WAV,IS_C_ORACLE
test $T2 ... ok

test result: ok. 1 passed; 0 failed;
EOF
check "skip-multivar" "SKIP missing_env=IS_PROBE_WAV,IS_C_ORACLE" "$(tierb_classify "$T2" "$TMP/skip2.log" 0)"

# 4. Skip wins over a missing ok line (marker is authoritative).
cat >"$TMP/skip-no-ok.log" <<EOF
running 1 test
TIERB_SKIP $S1 missing_env=IS_PROBE_WAV
EOF
check "skip-no-ok" "SKIP missing_env=IS_PROBE_WAV" "$(tierb_classify "$T1" "$TMP/skip-no-ok.log" 0)"

# 5. Failure: nonzero exit.
cat >"$TMP/fail.log" <<EOF
running 1 test
test $T1 ... FAILED

test result: FAILED. 0 passed; 1 failed;
EOF
check "fail-rc" "FAIL" "$(tierb_classify "$T1" "$TMP/fail.log" 1)"

# 6. Failure: exit 0 but no ok line (incomplete execution, no marker).
cat >"$TMP/no-ok.log" <<EOF
running 1 test
test $T1 ... ok
EOF
printf 'note: truncated before summary\n' >>"$TMP/no-ok.log"
# Strip the ok line to simulate a lost/foreign format.
grep -v " \.\.\. ok$" "$TMP/no-ok.log" >"$TMP/no-ok-stripped.log" || true
check "fail-no-ok-line" "FAIL" "$(tierb_classify "$T1" "$TMP/no-ok-stripped.log" 0)"

# 7. Another test's marker must not skip this test (short-name anchoring).
cat >"$TMP/other.log" <<EOF
running 1 test
TIERB_SKIP $S2 missing_env=IS_PROBE_WAV,IS_C_ORACLE
test $T1 ... ok

test result: ok. 1 passed; 0 failed;
EOF
check "other-marker-ignored" "PASS" "$(tierb_classify "$T1" "$TMP/other.log" 0)"

# 8. Summary exit codes: clean / incomplete / failed.
tierb_summarize 14 0 0 14 >/dev/null
check "summary-clean-rc" "0" "$?"
tierb_summarize 12 2 0 14 >/dev/null
check "summary-incomplete-rc" "2" "$?"
tierb_summarize 12 1 1 14 >/dev/null
check "summary-fail-rc" "1" "$?"
out=$(tierb_summarize 12 2 0 14)
check "summary-counts-line" "Tier-B: 12 passed, 2 skipped, 0 failed of 14 selected" "$out"

printf '%s: %d passed, %d failed\n' "$prog" "$pass" "$fail"
[ "$fail" -eq 0 ]
