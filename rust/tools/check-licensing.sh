#!/bin/sh
# check-licensing.sh — hygiene check for Ferrite licensing notices.
#
# Verifies package-level licensing files exist, the prominent
# Third-Party Modified Version notice is intact, and every
# rust/src/**/*.rs codec source carries a Ferrite/Third-Party
# Modified Version notice (accepting either the complete-block or
# the concise form).
#
# It also verifies the licensing boundary: rust/convert is
# independently developed converter code licensed MIT OR Apache-2.0,
# so it must declare that license, ship both license texts, and must
# NOT carry the FDK-derived codec notice.
#
# Scope: rust/src is codec code under FDK-AAC; rust/convert/src is
# independent code under MIT OR Apache-2.0. rust/build.rs, rust/tools,
# tools, tests, and fixtures are NOT checked for either header; their
# relationship to the codec was established separately (converter
# provenance: application code over the public encoder API, no
# FDK-derived tables or logic).
#
# This check is a text-match hygiene signal only. It does not prove
# legal compliance and never rewrites files. No network access.
#
# Usage:
#   rust/tools/check-licensing.sh
#
# Exit status: 0 when all checks pass; nonzero with a diagnostic
# naming the first failure class and the offending paths.

set -u

prog=$(basename -- "$0")

fail() {
    printf '%s: error: %s\n' "$prog" "$1" >&2
    exit "${2:-1}"
}

usage() {
    cat <<USAGE
usage: $prog

Offline read-only licensing hygiene check. See AGENTS.md
("Licensing and Third-Party Notices") for the policy.
USAGE
}

for arg in "$@"; do
    case "$arg" in
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage >&2
            fail "unknown argument: $arg (exit 2)" 2
            ;;
    esac
done

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || \
    fail "cannot resolve script directory"
# rust/tools/ -> rust/
CRATE_DIR=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P) || \
    fail "cannot resolve crate directory"
SRC_DIR="$CRATE_DIR/src"

[ -d "$SRC_DIR" ] || fail "codec source directory not found: $SRC_DIR"

# 1. Package-level files exist with the expected content.
for f in "$CRATE_DIR/NOTICE" "$CRATE_DIR/LICENSE-FDK-AAC"; do
    [ -f "$f" ] || fail "missing package-level licensing file: $f"
done

grep -q "Third-Party Modified Version" "$CRATE_DIR/NOTICE" || \
    fail "rust/NOTICE lacks the prominent Third-Party Modified Version notice"
grep -q "Modification notice date: 2026-10-26" "$CRATE_DIR/NOTICE" || \
    fail "rust/NOTICE lacks the modification notice date (2026-10-26)"
grep -q "Software License for The Fraunhofer" "$CRATE_DIR/NOTICE" || \
    fail "rust/NOTICE lacks the complete license text"
grep -q "Software License for The Fraunhofer" "$CRATE_DIR/LICENSE-FDK-AAC" || \
    fail "rust/LICENSE-FDK-AAC lacks the complete license text"

# Root NOTICE preserves the upstream text; presence check only.
# (Year strings are provenance; this check does not compare years.)
ROOT_NOTICE="$SCRIPT_DIR/../../NOTICE"
[ -f "$ROOT_NOTICE" ] || fail "missing root NOTICE: $ROOT_NOTICE"
grep -q "Software License for The Fraunhofer" "$ROOT_NOTICE" || \
    fail "root NOTICE lacks the upstream license text"

# 2. Manifest declares the project licensing approach.
grep -q 'license *= *"FDK-AAC"' "$CRATE_DIR/Cargo.toml" || \
    fail "rust/Cargo.toml does not declare license = \"FDK-AAC\""
grep -q 'license-file *= *"LICENSE-FDK-AAC"' "$CRATE_DIR/Cargo.toml" || \
    fail "rust/Cargo.toml does not set license-file = \"LICENSE-FDK-AAC\""

# 3. Every codec source carries a Ferrite modified-version notice.
# Accepts both the complete-block form and the concise form; any
# file containing the designation string passes. New files that
# fail this check need either the notice or an explicitly
# documented exemption (do not add exemptions silently).
missing=""
count=0
for rs in $(find "$SRC_DIR" -name '*.rs' -print | sort); do
    count=$((count + 1))
    if ! grep -q "Third-Party Modified Version" "$rs"; then
        missing="$missing
$rs"
    fi
done

[ "$count" -gt 0 ] || fail "no .rs files found under $SRC_DIR"

if [ -n "$missing" ]; then
    printf '%s: error: %d codec source file(s) under rust/src lack a Ferrite/Third-Party Modified Version notice:\n' \
        "$prog" "$(printf '%s' "$missing" | grep -c /)" >&2
    printf '%s\n' "$missing" >&2
    printf '%s: hint: add the concise codec notice or document an exemption per AGENTS.md; do not copy the full license block into new files.\n' "$prog" >&2
    exit 1
fi

printf 'licensing hygiene OK: NOTICE + LICENSE-FDK-AAC present, manifest declares FDK-AAC, %d rust/src file(s) carry the modified-version notice\n' "$count"

# 4. Converter licensing boundary: independently developed code under
# MIT OR Apache-2.0, not FDK-AAC.
CONVERT_DIR="$CRATE_DIR/convert"
CONVERT_MANIFEST="$CONVERT_DIR/Cargo.toml"
[ -f "$CONVERT_MANIFEST" ] || fail "missing converter manifest: $CONVERT_MANIFEST"

grep -q 'license *= *"MIT OR Apache-2.0"' "$CONVERT_MANIFEST" || \
    fail "rust/convert/Cargo.toml does not declare license = \"MIT OR Apache-2.0\""
if grep -q 'license *= *"FDK-AAC"' "$CONVERT_MANIFEST"; then
    fail "rust/convert/Cargo.toml must not declare the FDK-AAC license for independent converter code"
fi

for f in "$CONVERT_DIR/LICENSE-MIT" "$CONVERT_DIR/LICENSE-APACHE" "$CONVERT_DIR/THIRD-PARTY-NOTICES.md"; do
    [ -f "$f" ] || fail "missing converter licensing file: $f"
done

grep -q "Permission is hereby granted, free of charge" "$CONVERT_DIR/LICENSE-MIT" || \
    fail "rust/convert/LICENSE-MIT lacks the MIT license text"
grep -q "Apache License" "$CONVERT_DIR/LICENSE-APACHE" || \
    fail "rust/convert/LICENSE-APACHE lacks the Apache license text"
grep -q "Version 2.0, January 2004" "$CONVERT_DIR/LICENSE-APACHE" || \
    fail "rust/convert/LICENSE-APACHE lacks the Apache 2.0 version marker"

# No FDK-derived codec notice may appear in independently developed
# converter sources.
stray=""
for rs in $(find "$CONVERT_DIR/src" -name '*.rs' -print | sort); do
    if grep -q "Third-Party Modified Version" "$rs"; then
        stray="$stray
$rs"
    fi
done

if [ -n "$stray" ]; then
    printf '%s: error: %d converter source file(s) under rust/convert/src carry the FDK-derived codec notice:\n' \
        "$prog" "$(printf '%s' "$stray" | grep -c /)" >&2
    printf '%s\n' "$stray" >&2
    printf '%s: hint: converter sources are independent MIT OR Apache-2.0 code; do not copy FDK-derived headers into them.\n' "$prog" >&2
    exit 1
fi

printf 'converter licensing OK: MIT OR Apache-2.0 declared, both texts present, no FDK-derived notices under rust/convert/src\n'
exit 0
