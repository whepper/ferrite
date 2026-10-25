#!/bin/sh
# check-reference.sh — validate an FDK AAC reference checkout.
#
# Compares a local reference checkout against the pinned revision recorded
# in docs/reference/fdk-aac-reference-revision.txt and checks that key
# reference source files are present.
#
# This script validates source checkout identity and completeness ONLY.
# It does not require a built library, a built enc_oracle, an AAC decoder,
# a C/C++ compiler, or CMake. A valid pinned source checkout must pass
# this script before it is built.
#
# Usage:
#   rust/tools/check-reference.sh --ref-dir PATH
#   FDK_REF_DIR=PATH rust/tools/check-reference.sh
#
# Exit status: 0 when the checkout matches the pinned revision and all
# required source files exist; nonzero otherwise.

set -u

prog=$(basename -- "$0")

usage() {
    cat <<USAGE
usage: $prog [--ref-dir PATH] [--pin-file PATH]

Validates an FDK AAC reference checkout against the pinned revision in
docs/reference/fdk-aac-reference-revision.txt.

  --ref-dir PATH   path to the reference checkout (default: \$FDK_REF_DIR)
  --pin-file PATH  path to the pin file (default: derived from this
                   script's location inside the Ferrite checkout)
  -h, --help       print this message and exit

Failure behavior: prints a diagnostic naming the missing directory,
non-Git checkout, unreadable/malformed pin, revision mismatch (with
expected and actual revisions plus a recovery hint), or missing
required source file, and exits nonzero.
USAGE
}

fail() {
    printf '%s: error: %s\n' "$prog" "$1" >&2
    exit "${2:-1}"
}

REF_DIR=${FDK_REF_DIR:-}
PIN_FILE=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --ref-dir)
            [ "$#" -ge 2 ] || { usage >&2; fail "missing value for --ref-dir (exit 2)" 2; }
            REF_DIR=$2
            shift 2
            ;;
        --ref-dir=*)
            REF_DIR=${1#--ref-dir=}
            shift
            ;;
        --pin-file)
            [ "$#" -ge 2 ] || { usage >&2; fail "missing value for --pin-file (exit 2)" 2; }
            PIN_FILE=$2
            shift 2
            ;;
        --pin-file=*)
            PIN_FILE=${1#--pin-file=}
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

# Default pin file: docs/reference/fdk-aac-reference-revision.txt in the
# Ferrite checkout containing this script (rust/tools/ -> repo root).
if [ -z "$PIN_FILE" ]; then
    SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || \
        fail "cannot resolve script directory"
    PIN_FILE="$SCRIPT_DIR/../../docs/reference/fdk-aac-reference-revision.txt"
fi

[ -n "$REF_DIR" ] || {
    usage >&2
    fail "no reference directory: pass --ref-dir PATH or set FDK_REF_DIR (exit 2)" 2
}

REF_ABS=$(CDPATH= cd -- "$REF_DIR" 2>/dev/null && pwd -P) || \
    fail "not a directory: $REF_DIR"

[ -f "$PIN_FILE" ] || \
    fail "pin file not found: $PIN_FILE"

# The pin file is the single source of truth for the expected revision.
PIN=$(sed -n 's/^Pinned commit: `\([0-9a-f][0-9a-f]*\)`.*/\1/p' "$PIN_FILE" | head -n 1)
case "$PIN" in
    ""|*[!0-9a-f]*)
        fail "malformed pin file (no 'Pinned commit: \`<sha>\`' line): $PIN_FILE"
        ;;
esac
if [ "${#PIN}" -ne 40 ]; then
    fail "malformed pin file (expected 40-hex revision, got '$PIN'): $PIN_FILE"
fi

ACTUAL=$(git -C "$REF_ABS" rev-parse HEAD 2>/dev/null) || \
    fail "not a Git checkout (or git unavailable): $REF_ABS"

if [ "$ACTUAL" != "$PIN" ]; then
    printf '%s: error: revision mismatch for %s\n' "$prog" "$REF_ABS" >&2
    printf '  expected (pin file): %s\n' "$PIN" >&2
    printf '  actual (checkout):   %s\n' "$ACTUAL" >&2
    printf '  hint: git -C "%s" fetch <remote> && git -C "%s" checkout %s\n' \
        "$REF_ABS" "$REF_ABS" "$PIN" >&2
    exit 1
fi

# Key reference sources required by oracle and build workflows.
for rel in \
    cpp/libAACenc/src/aacEnc_rom.cpp \
    tools/enc-oracle/enc_oracle.cpp \
    tools/enc-oracle/extract_testdata.py \
    CMakeLists.txt \
    wavreader.c \
    NOTICE \
; do
    [ -f "$REF_ABS/$rel" ] || \
        fail "missing required source file: $rel (under $REF_ABS)"
done

printf 'reference checkout OK: %s at %s\n' "$REF_ABS" "$PIN"
exit 0
