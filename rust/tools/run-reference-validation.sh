#!/bin/sh
# run-reference-validation.sh — local driver for explicitly requested
# FDK AAC reference validation.
#
# Orchestrates the established reference-validation steps without making
# ordinary Rust-only development depend on them. Without --validate this
# script does nothing except say so and exits 0.
#
# Usage:
#   rust/tools/run-reference-validation.sh --validate [--ref-dir PATH]
#       [--ref-build DIR] [--out-dir DIR] [--skip-build]
#
#   --validate    opt in to reference validation (required for anything
#                 beyond the not-requested notice)
#   --ref-dir     reference checkout (default: $FDK_REF_DIR, required
#                 when validating)
#   --ref-build   reference build directory (default: $OUT_DIR/ref-build,
#                 kept outside both repositories)
#   --out-dir     fixtures, dumps, C outputs, logs (default: a fresh
#                 directory under $TMPDIR; always outside tracked trees)
#   --skip-build  reuse an existing reference build after verifying its
#                 artifacts instead of reconfiguring and rebuilding
#   -h, --help    print this message and exit
#
# What it does, in order: validate checkout (check-reference.sh) ->
# build reference lib + helpers (cmake) -> build enc_oracle (documented
# flags, reference root as CWD) -> fixtures + oracle dumps -> C-encoded
# reference outputs -> preflight-validation.sh --require -> the Tier-B
# Rust tests (one cargo invocation per test, exact paths) -> config-only
# extraction byte-compared against committed cfg_*.txt.
#
# Per-test results distinguish PASS, SKIP (probe opt-in inputs absent; the
# probe printed a TIERB_SKIP marker and validated nothing), and FAIL. A
# SKIP is never counted as passed: any skipped selected test ends the run
# INCOMPLETE (exit 2), naming the probe and the missing input. The M511
# profiling workload is genuinely optional and joins the inventory only
# when M511_PROFILE is set.
#
# Full oracle re-extraction and encoded-payload bit identity are
# deliberately NOT part of this driver: their determinism has not been
# established (see tools/enc-oracle/README.md).
set -u

prog=$(basename -- "$0")

usage() {
    cat <<USAGE
usage: $prog --validate [--ref-dir PATH] [--ref-build DIR] [--out-dir DIR] [--skip-build]

Local driver for explicitly requested FDK AAC reference validation.

  --validate    opt in (without it: prints "not requested" and exits 0)
  --ref-dir     reference checkout (default: \$FDK_REF_DIR)
  --ref-build   reference build dir (default: \$OUT_DIR/ref-build)
  --out-dir     scratch root for fixtures, dumps, outputs, logs
                (default: fresh dir under \$TMPDIR)
  --skip-build  verify existing build artifacts instead of rebuilding
  -h, --help    print this message and exit
USAGE
}

fail() {
    printf '%s: error: %s\n' "$prog" "$1" >&2
    exit "${2:-1}"
}

log_tail() {
    printf '%s: log: %s (last lines follow)\n' "$prog" "$1" >&2
    tail -n 20 "$1" >&2 || true
}

VALIDATE=0
SKIP_BUILD=0
REF_DIR=${FDK_REF_DIR:-}
REF_BUILD=""
OUT_DIR=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --validate)
            VALIDATE=1
            shift
            ;;
        --skip-build)
            SKIP_BUILD=1
            shift
            ;;
        --ref-dir)
            [ "$#" -ge 2 ] || { usage >&2; fail "missing value for --ref-dir (exit 2)" 2; }
            REF_DIR=$2
            shift 2
            ;;
        --ref-dir=*)
            REF_DIR=${1#--ref-dir=}
            shift
            ;;
        --ref-build)
            [ "$#" -ge 2 ] || { usage >&2; fail "missing value for --ref-build (exit 2)" 2; }
            REF_BUILD=$2
            shift 2
            ;;
        --ref-build=*)
            REF_BUILD=${1#--ref-build=}
            shift
            ;;
        --out-dir)
            [ "$#" -ge 2 ] || { usage >&2; fail "missing value for --out-dir (exit 2)" 2; }
            OUT_DIR=$2
            shift 2
            ;;
        --out-dir=*)
            OUT_DIR=${1#--out-dir=}
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

if [ "$VALIDATE" -eq 0 ]; then
    printf '%s: reference validation not requested (pass --validate to opt in); nothing was built or run\n' "$prog"
    exit 0
fi

[ -n "$REF_DIR" ] || \
    fail "FDK_REF_DIR is not set and --ref-dir was not given; point it at the pinned reference checkout (see docs/reference/fdk-aac-reference-revision.txt)"

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P) || \
    fail "cannot resolve script directory"
# PASS/SKIP/FAIL classification shared with test-tierb-report.sh.
. "$SCRIPT_DIR/tierb-report.sh" || \
    fail "cannot load tierb-report.sh beside $0"
FERRITE_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd -P) || \
    fail "cannot resolve Ferrite root"
# Single source for scratch paths; plain $TMPDIR is unset on some systems
# (notably Linux CI) and would abort under `set -u`.
TMPDIR_SAFE=${TMPDIR:-/tmp}

if [ -z "$OUT_DIR" ]; then
    OUT_DIR=$(mktemp -d "$TMPDIR_SAFE/ferrite-refval.XXXXXX") || \
        fail "cannot create scratch directory"
fi
if [ -z "$REF_BUILD" ]; then
    REF_BUILD="$OUT_DIR/ref-build"
fi
# Resolve without creating anything, so a rejected path leaves no trace.
# (OUT_DIR/REF_BUILD typically do not exist yet; resolve via the parent.)
canonical() {
    if [ -d "$1" ]; then
        CDPATH= cd -- "$1" && pwd -P
    else
        _canon_parent=$(dirname -- "$1") || return 1
        _canon_base=$(basename -- "$1") || return 1
        [ -n "$_canon_base" ] && [ "$_canon_base" != "/" ] || return 1
        _canon_pabs=$(CDPATH= cd -- "$_canon_parent" 2>/dev/null && pwd -P) || return 1
        printf '%s/%s\n' "$_canon_pabs" "$_canon_base"
    fi
}
OUT_ABS=$(canonical "$OUT_DIR") || \
    fail "cannot resolve output directory $OUT_DIR (its parent must exist)"
BUILD_ABS=$(canonical "$REF_BUILD") || \
    fail "cannot resolve build directory $REF_BUILD (its parent must exist)"
REF_ABS=$(canonical "$REF_DIR") || \
    fail "cannot resolve reference directory $REF_DIR"
# Scratch and build outputs must resolve outside both the Ferrite checkout
# and the reference checkout (explicit user paths included); nothing here
# may dirty either repository.
for _d in "$OUT_ABS" "$BUILD_ABS"; do
    case "$_d/" in
        "$FERRITE_ROOT/"*|"$REF_ABS/"*)
            fail "output/build directory $_d resolves inside a repository ($FERRITE_ROOT or $REF_ABS); choose a path outside both checkouts" ;;
    esac
done
# Use the canonical absolute forms from here on: later stages run stage-local
# `cd` commands, under which the raw user-supplied relative paths would
# silently re-resolve to different directories.
OUT_DIR=$OUT_ABS
REF_BUILD=$BUILD_ABS
REF_DIR=$REF_ABS
mkdir -p "$OUT_DIR/fixtures" "$OUT_DIR/oracle" "$OUT_DIR/cref" \
         "$OUT_DIR/m55" "$OUT_DIR/cfgtest" "$OUT_DIR/logs" || \
    fail "cannot create scratch subdirectories under $OUT_DIR"
mkdir -p "$REF_BUILD" || fail "cannot create build directory $REF_BUILD"

LOGDIR="$OUT_DIR/logs"
printf '%s: out dir: %s\n' "$prog" "$OUT_DIR"
printf '%s: ferrite root: %s\n' "$prog" "$FERRITE_ROOT"

# ---- Tool preflight (only tools this path needs) ----
need_tool() {
    command -v "$1" >/dev/null 2>&1 || fail "required tool missing: $1 ($2)"
}
need_tool git "needed to validate the reference checkout"
need_tool c++ "needed to build enc_oracle"
need_tool python3 "needed for fixture generation and config extraction"
if [ "$SKIP_BUILD" -eq 0 ]; then
    need_tool cmake "needed to configure and build the reference library"
fi

# ---- Stage 0: checkout identity (before any build) ----
"$SCRIPT_DIR/check-reference.sh" --ref-dir "$REF_DIR" >"$LOGDIR/checkout.log" 2>&1
rc=$?
cat "$LOGDIR/checkout.log"
[ "$rc" -eq 0 ] || fail "reference checkout invalid; see $LOGDIR/checkout.log"

# CMake flags used for the reference build. The same string is recorded in
# the build stamp and compared on --skip-build, so keep both in sync by
# editing here only. (Unquoted expansions of it below are intentional.)
CMAKE_ARGS="-DCMAKE_BUILD_TYPE=Release -DBUILD_PROGRAMS=ON"

# ---- Stage 1: reference library + helper programs ----
if [ "$SKIP_BUILD" -eq 1 ]; then
    stamp="$REF_BUILD/reference-build-info.txt"
    [ -f "$stamp" ] || \
        fail "--skip-build given but $REF_BUILD has no reference-build-info.txt stamp; rebuild without --skip-build"
    # Read-only parsing (never source the stamp): missing keys mean a
    # foreign or malformed stamp.
    stamp_dir=$(sed -n 's/^ref_dir=//p' "$stamp" | head -n 1)
    stamp_rev=$(sed -n 's/^ref_rev=//p' "$stamp" | head -n 1)
    stamp_args=$(sed -n 's/^cmake_args=//p' "$stamp" | head -n 1)
    stamp_build=$(sed -n 's/^build_dir=//p' "$stamp" | head -n 1)
    if [ -z "$stamp_dir" ] || [ -z "$stamp_rev" ] || \
       [ -z "$stamp_args" ] || [ -z "$stamp_build" ]; then
        fail "--skip-build given but stamp $stamp is malformed; rebuild without --skip-build"
    fi
    [ "$stamp_dir" = "$REF_ABS" ] || \
        fail "--skip-build stamp refers to checkout $stamp_dir, not $REF_ABS; rebuild without --skip-build"
    cur_rev=$(git -C "$REF_DIR" rev-parse HEAD 2>/dev/null) || \
        fail "cannot read reference revision for stamp check"
    [ "$stamp_rev" = "$cur_rev" ] || \
        fail "--skip-build stamp is stale (built from $stamp_rev, checkout is at $cur_rev); rebuild without --skip-build"
    [ "$stamp_args" = "$CMAKE_ARGS" ] || \
        fail "--skip-build stamp has different cmake args ($stamp_args); rebuild without --skip-build"
    [ "$stamp_build" = "$BUILD_ABS" ] || \
        fail "--skip-build stamp refers to build dir $stamp_build, not $BUILD_ABS; rebuild without --skip-build"
    for _bin in aac-enc aac-dec; do
        [ -x "$REF_BUILD/$_bin" ] || \
            fail "--skip-build given but $REF_BUILD/$_bin is missing or not executable; rebuild without --skip-build"
    done
    if ! ls "$REF_BUILD"/libfdk-aac.* >/dev/null 2>&1; then
        fail "--skip-build given but $REF_BUILD lacks libfdk-aac.*; rebuild without --skip-build"
    fi
    printf '%s: reusing existing build in %s (stamp rev %s)\n' "$prog" "$REF_BUILD" "$stamp_rev"
else
    # shellcheck disable=SC2086: CMAKE_ARGS word-splitting is intentional.
    cmake -S "$REF_DIR" -B "$REF_BUILD" $CMAKE_ARGS >"$LOGDIR/cmake-configure.log" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        log_tail "$LOGDIR/cmake-configure.log"
        fail "cmake configure failed"
    fi
    cmake --build "$REF_BUILD" -j 8 >"$LOGDIR/cmake-build.log" 2>&1
    rc=$?
    if [ "$rc" -ne 0 ]; then
        log_tail "$LOGDIR/cmake-build.log"
        fail "cmake build failed"
    fi
    # Stamp the build with its provenance. This prevents accidental reuse of
    # artifacts from a different checkout or configuration; it does not prove
    # binaries have not been manually replaced afterwards.
    stamp_rev=$(git -C "$REF_DIR" rev-parse HEAD 2>/dev/null) || \
        fail "cannot read reference revision for build stamp"
    [ -n "$stamp_rev" ] || fail "cannot record empty reference revision in build stamp"
    {
        printf '# generated by run-reference-validation.sh; do not edit\n'
        printf 'ref_dir=%s\n' "$REF_ABS"
        printf 'ref_rev=%s\n' "$stamp_rev"
        printf 'cmake_args=%s\n' "$CMAKE_ARGS"
        printf 'build_dir=%s\n' "$BUILD_ABS"
    } >"$REF_BUILD/reference-build-info.txt" || \
        fail "cannot write build stamp in $REF_BUILD"
fi

# ---- Stage 2: enc_oracle (documented flags, reference root as CWD) ----
(
    cd "$REF_DIR" && \
    c++ -std=c++11 -O2 \
        -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include \
        -I cpp/libMpegTPEnc/include -I . \
        tools/enc-oracle/enc_oracle.cpp wavreader.c \
        -L "$REF_BUILD" -lfdk-aac -Wl,-rpath,"$REF_BUILD" \
        -o "$REF_BUILD/enc_oracle"
) >"$LOGDIR/enc-oracle-build.log" 2>&1
rc=$?
if [ "$rc" -ne 0 ]; then
    log_tail "$LOGDIR/enc-oracle-build.log"
    fail "enc_oracle build failed"
fi

# ---- Stage 3: fixtures + oracle dumps (scratch only) ----
python3 "$REF_DIR/tools/enc-oracle/make_fixtures.py" \
    "$OUT_DIR/fixtures" >"$LOGDIR/fixtures.log" 2>&1
rc=$?
if [ "$rc" -ne 0 ]; then
    log_tail "$LOGDIR/fixtures.log"
    fail "fixture generation failed"
fi
: >"$LOGDIR/dumps.log"
for stem in m2-onset-stereo-48k m2-stepdown-stereo-48k m2-click-mono-48k \
            m2-edge-mono-48k m2-silence-stereo-48k m2-sine-stereo-48k \
            m2-impulse-mono-48k m3-multitone-stereo-48k m3-sweep-stereo-48k \
            m3-noise-mono-48k m3-tonal-noise-stereo-48k \
            m3-quiet-sine-stereo-48k m3-sine-stereo-441k m3-onset-stereo-441k \
            m6-stress-stereo-48k; do
    "$REF_BUILD/enc_oracle" "$OUT_DIR/fixtures/$stem.wav" \
        "$OUT_DIR/oracle/$stem.oracle.txt" >>"$LOGDIR/dumps.log" 2>&1 || \
        { log_tail "$LOGDIR/dumps.log"; fail "oracle dump failed for $stem"; }
    cp "$OUT_DIR/oracle/$stem.oracle.txt" "$OUT_DIR/oracle/$stem.s1.txt" || \
        fail "cannot stage $stem.s1.txt"
done

# ---- Stage 4: C-encoded reference outputs (scratch only) ----
# PHASE1_C_DIR: the 15 phase-1 stems encoded with the reference aac-enc.
# M55_C_DIR: the 4 M52 music/speech fixtures from committed testdata.
: >"$LOGDIR/c-encode.log"
for stem in m2-onset-stereo-48k m2-stepdown-stereo-48k m2-click-mono-48k \
            m2-edge-mono-48k m2-silence-stereo-48k m2-sine-stereo-48k \
            m2-impulse-mono-48k m3-multitone-stereo-48k m3-sweep-stereo-48k \
            m3-noise-mono-48k m3-tonal-noise-stereo-48k \
            m3-quiet-sine-stereo-48k m3-sine-stereo-441k m3-onset-stereo-441k \
            m6-stress-stereo-48k; do
    "$REF_BUILD/aac-enc" "$OUT_DIR/fixtures/$stem.wav" \
        "$OUT_DIR/cref/$stem.aac" >>"$LOGDIR/c-encode.log" 2>&1 || \
        { log_tail "$LOGDIR/c-encode.log"; fail "reference encode failed for $stem"; }
done
for stem in m7-music-stereo-48k m7-speech-pauses-stereo-48k \
            m7-speech-clean-stereo-48k m8-music-eroica-stereo-48k; do
    "$REF_BUILD/aac-enc" \
        "$FERRITE_ROOT/rust/src/aac_enc/testdata/$stem.wav" \
        "$OUT_DIR/m55/$stem.aac" >>"$LOGDIR/c-encode.log" 2>&1 || \
        { log_tail "$LOGDIR/c-encode.log"; fail "reference encode failed for $stem"; }
done

# ---- Stage 5: preflight with the provisioned environment ----
FDK_REF_DIR="$REF_DIR" \
PHASE1_ORACLE_DIR="$OUT_DIR/oracle" \
PHASE1_C_DIR="$OUT_DIR/cref" \
M55_C_DIR="$OUT_DIR/m55" \
AAC_DEC_BIN="$REF_BUILD/aac-dec" \
    "$SCRIPT_DIR/preflight-validation.sh" --require >"$LOGDIR/preflight.log" 2>&1
rc=$?
cat "$LOGDIR/preflight.log"
[ "$rc" -eq 0 ] || fail "preflight failed; see $LOGDIR/preflight.log"

# ---- Stage 6: Tier-B tests, one exact invocation each ----
# Inventory mirrors the CI Tier-A skip list; --exact full paths avoid any
# reliance on multi-filter OR semantics. The per-test cargo invocation
# passes --nocapture so TIERB_SKIP markers (probes whose opt-in inputs are
# absent) reach the log; tierb_classify maps each test to PASS, SKIP, or
# FAIL, and a SKIP never counts as passed. M511_* stays genuinely
# optional: the profiling workload joins the inventory only when
# M511_PROFILE is set.
TIER_B="aac_enc::m5a_decode::m5a_tests::m5a_decode_check
aac_enc::stream_driver::m52_tests::m55_streams_match_c_reference
aac_enc::stream_driver::phase1_tests::phase1_frontend_agreement_sine_f5
aac_enc::stream_driver::phase1_tests::phase1e_sine_f5_band16_residue
aac_enc::stream_driver::phase1_tests::phase1e_onset_f24_bands_causality
aac_enc::stream_driver::phase1_tests::phase1f_multitone_f1_no_live_divergence
aac_enc::stream_driver::phase1_tests::phase1f_stepdown_f1_no_live_divergence
aac_enc::stream_driver::phase1_tests::phase1g_long_flips_residue
aac_enc::stream_driver::phase1_tests::phase1i_sine44_f5_tier_b
aac_enc::stream_driver::phase1_tests::phase1i_multitone_f5_tier_b
aac_enc::stream_driver::phase1_tests::phase1_full_stream_parity
aac_enc::stream_driver::is_integration_tests::intensity_probe_c_oracle_parity
aac_enc::stream_driver::is_integration_tests::intensity_probe_wav_parity
aac_enc::stream_driver::is_integration_tests::intensity_probe_adts_decodes"
if [ -n "${M511_PROFILE-}" ]; then
    TIER_B="$TIER_B
aac_enc::stream_driver::m511_tests::m511_profile_loop"
fi

EXPECTED=$(printf '%s\n' $TIER_B | grep -c .)
[ "$EXPECTED" -gt 0 ] || fail "Tier-B inventory is empty"
ran=0
passed=0
skipped=0
failed=0
skip_details=""
: >"$LOGDIR/tests.log"
for t in $TIER_B; do
    ran=$((ran + 1))
    printf '%s: [%d/%d] %s\n' "$prog" "$ran" "$EXPECTED" "$t"
    seg_from=$(wc -l <"$LOGDIR/tests.log")
    if ! (
        cd "$FERRITE_ROOT/rust" && \
        FDK_REF_DIR="$REF_DIR" \
        PHASE1_ORACLE_DIR="$OUT_DIR/oracle" \
        PHASE1_C_DIR="$OUT_DIR/cref" \
        M55_C_DIR="$OUT_DIR/m55" \
        AAC_DEC_BIN="$REF_BUILD/aac-dec" \
        cargo test --locked --features aac_enc --lib -- --exact "$t" --nocapture \
            >>"$LOGDIR/tests.log" 2>&1
    ); then
        cargo_rc=1
    else
        cargo_rc=0
    fi
    seg="$LOGDIR/tierb-seg.log"
    tail -n +"$((seg_from + 1))" "$LOGDIR/tests.log" >"$seg" || \
        fail "cannot slice per-test log segment for $t"
    verdict=$(tierb_classify "$t" "$seg" "$cargo_rc")
    case "$verdict" in
        PASS)
            passed=$((passed + 1))
            ;;
        SKIP*)
            skipped=$((skipped + 1))
            skip_details="${skip_details}  SKIP $t ${verdict#SKIP }
"
            ;;
        *)
            printf '%s: FAILED %s (see %s/tests.log)\n' "$prog" "$t" "$LOGDIR" >&2
            failed=$((failed + 1))
            ;;
    esac
done
if [ "$ran" -ne "$EXPECTED" ]; then
    fail "Tier-B selection incomplete: ran $ran, expected $EXPECTED"
fi
tierb_summarize "$passed" "$skipped" "$failed" "$EXPECTED"
summary_rc=$?
if [ -n "$skip_details" ]; then
    printf '%s: skipped (not executed), reasons:\n%s' "$prog" "$skip_details" >&2
fi
if [ "$summary_rc" -eq 1 ]; then
    fail "$failed of $ran Tier-B tests failed; see $LOGDIR/tests.log"
fi
if [ "$summary_rc" -eq 2 ]; then
    fail "Tier-B INCOMPLETE: $skipped of $ran selected tests skipped (not executed); provision the missing opt-in inputs and re-run (exit 2)" 2
fi
printf '%s: Tier-B tests: %d/%d passed\n' "$prog" "$ran" "$EXPECTED"

# ---- Stage 7: config-only extraction, byte-compared ----
# The 4th snapshot (44100/112000) needs the M5.b input dump plus an
# explicit stem (see tools/enc-oracle/README.md "Config-only regeneration").
(
    cd "$FERRITE_ROOT/rust" && \
    cargo test --locked --features aac_enc --lib -- --exact \
        aac_enc::m5b_measure::m5b_tests::m5b_dump_inputs \
        >>"$LOGDIR/tests.log" 2>&1
) || { log_tail "$LOGDIR/tests.log"; fail "m5b_dump_inputs failed"; }
"$REF_BUILD/enc_oracle" \
    "$TMPDIR_SAFE/m5b-quality/inputs/onset-mono-44100.orig.wav" \
    "$OUT_DIR/oracle/onset-mono-44100.oracle.txt" >>"$LOGDIR/dumps.log" 2>&1 || \
    { log_tail "$LOGDIR/dumps.log"; fail "oracle dump failed for onset-mono-44100"; }
python3 "$REF_DIR/tools/enc-oracle/extract_testdata.py" --cfg-only \
    "$OUT_DIR/oracle" "$OUT_DIR/cfgtest" onset-mono-44100 \
    >>"$LOGDIR/cfgtest.log" 2>&1 || \
    { log_tail "$LOGDIR/cfgtest.log"; fail "config-only extraction failed"; }
for cfg in cfg_44100_96000 cfg_44100_112000 cfg_48000_96000 cfg_48000_112000; do
    if ! cmp -s "$OUT_DIR/cfgtest/$cfg.txt" \
                "$FERRITE_ROOT/rust/src/aac_enc/testdata/$cfg.txt"; then
        fail "config snapshot mismatch: $cfg.txt differs from committed testdata"
    fi
done
printf '%s: config-only snapshots byte-identical (4/4)\n' "$prog"

printf '%s: reference validation complete: Tier-B %d/%d, cfg 4/4; out dir: %s\n' \
    "$prog" "$ran" "$EXPECTED" "$OUT_DIR"
