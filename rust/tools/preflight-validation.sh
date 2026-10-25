#!/bin/sh
# preflight-validation.sh — preflight for explicitly-requested reference validation.
#
# Distinguishes three states:
#   1. reference validation not requested (no Tier-B variables set and no
#      --require flag): prints a note and exits 0. Ordinary cargo build,
#      test, and package never need the reference checkout.
#   2. validation requested but inputs missing: fails naming the missing
#      artifact and how to supply or build it.
#   3. validation requested and inputs available: exits 0.
#
# Requested means any of PHASE1_ORACLE_DIR, PHASE1_C_DIR, M55_C_DIR,
# AAC_DEC_BIN, IS_PROBE_WAV, IS_C_ORACLE, M511_PROFILE is set (non-empty),
# or --require is passed. The checkout itself is validated with
# check-reference.sh against docs/reference/fdk-aac-reference-revision.txt;
# this script never duplicates the pinned SHA.
#
# usage:
#   rust/tools/preflight-validation.sh [--require]
#
# This script checks that inputs exist. It does not build the library,
# run the oracle, or execute tests.

set -u

prog=$(basename -- "$0")

usage() {
    cat <<USAGE
usage: $prog [--require]

Preflight for explicitly-requested FDK AAC reference validation.

  --require    treat validation as requested even when no Tier-B
               environment variable is set (checkout identity is
               still validated)
  -h, --help   print this message and exit

Without --require and with no Tier-B variables set
(PHASE1_ORACLE_DIR, PHASE1_C_DIR, M55_C_DIR, AAC_DEC_BIN,
IS_PROBE_WAV, IS_C_ORACLE, M511_PROFILE), validation is not
requested: prints a note and exits 0.
USAGE
}

fail() {
    printf '%s: error: %s\n' "$prog" "$1" >&2
    exit "${2:-1}"
}

REQUIRE=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --require)
            REQUIRE=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            fail "unknown argument: $1 (exit 2)" 2
            ;;
    esac
done

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || \
    fail "cannot resolve script directory"

requested=0
if [ "$REQUIRE" -eq 1 ]; then
    requested=1
else
    for v in PHASE1_ORACLE_DIR PHASE1_C_DIR M55_C_DIR AAC_DEC_BIN \
             IS_PROBE_WAV IS_C_ORACLE M511_PROFILE; do
        eval "val=\${$v-}"
        if [ -n "$val" ]; then
            requested=1
            break
        fi
    done
fi

if [ "$requested" -eq 0 ]; then
    printf '%s: reference validation not requested (no Tier-B variables set); skipping\n' "$prog"
    exit 0
fi

[ -n "${FDK_REF_DIR:-}" ] || \
    fail "FDK_REF_DIR is not set: reference validation was requested but no checkout was given; set FDK_REF_DIR to a reference checkout at the pinned revision (see docs/reference/fdk-aac-reference-revision.txt)"

"$SCRIPT_DIR/check-reference.sh" --ref-dir "$FDK_REF_DIR" || \
    fail "reference checkout invalid; see diagnostics above"

if [ -n "${PHASE1_ORACLE_DIR-}" ]; then
    [ -d "$PHASE1_ORACLE_DIR" ] || \
        fail "PHASE1_ORACLE_DIR=$PHASE1_ORACLE_DIR is not a directory: generate oracle dumps per tools/enc-oracle/README.md (\"Fixtures and runs\") and set PHASE1_ORACLE_DIR to the dump directory"
fi
if [ -n "${PHASE1_C_DIR-}" ]; then
    [ -d "$PHASE1_C_DIR" ] || \
        fail "PHASE1_C_DIR=$PHASE1_C_DIR is not a directory: build the reference library and encode the fixtures with its aac-enc into that directory"
fi
if [ -n "${M55_C_DIR-}" ]; then
    [ -d "$M55_C_DIR" ] || \
        fail "M55_C_DIR=$M55_C_DIR is not a directory: encode the M52 fixtures with the reference aac-enc into \$M55_C_DIR as <stem>.aac"
fi
if [ -n "${AAC_DEC_BIN-}" ]; then
    [ -x "$AAC_DEC_BIN" ] || \
        fail "AAC_DEC_BIN=$AAC_DEC_BIN is missing or not executable: build it with cmake -S <ref> -B <bld> -DBUILD_PROGRAMS=ON -DCMAKE_BUILD_TYPE=Release && cmake --build <bld> --target aac-dec"
fi
if [ -n "${IS_PROBE_WAV-}" ]; then
    [ -f "$IS_PROBE_WAV" ] || \
        fail "IS_PROBE_WAV=$IS_PROBE_WAV is not a file: point it at the retained probe WAV"
fi
if [ -n "${IS_C_ORACLE-}" ]; then
    [ -f "$IS_C_ORACLE" ] || \
        fail "IS_C_ORACLE=$IS_C_ORACLE is not a file: point it at the C oracle transcript for the probe"
fi

printf '%s: reference validation inputs OK (FDK_REF_DIR=%s)\n' "$prog" "$FDK_REF_DIR"
exit 0
