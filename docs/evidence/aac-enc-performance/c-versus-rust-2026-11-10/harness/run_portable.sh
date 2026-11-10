#!/bin/bash
# Portable-build C-vs-Rust paired check (generic CPU targets).
# 8 alternating pairs, 10 measured repetitions + 1 warm-up, full 20-workload corpus.
set -e
T=${T:-/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10}
ORDER="$T/run-order-portable.tsv"
: > "$ORDER"

run_cp() {
    tag="$1"
    log="$T/logs/cp-$tag.log"
    out="$T/out/cp-$tag"
    rm -rf "$out"; mkdir -p "$out"
    "$T/bin/c_bench_corpus_portable" "$T/corpus-manifest.tsv" "$out" 10 1 > "$log" 2>&1
    rc=$?
    rows=$(grep -c '^ROW' "$log")
    printf 'cp\t%s\t%s\t%s\n' "$tag" "$rc" "$rows" >> "$ORDER"
    [ "$rc" -eq 0 ] && [ "$rows" -eq 200 ] || { echo "FAIL cp $tag rc=$rc rows=$rows" >&2; exit 1; }
    echo "OK cp $tag"
}

run_rp() {
    tag="$1"
    log="$T/logs/rp-$tag.log"
    out="$T/out/rp-$tag"
    rm -rf "$out"; mkdir -p "$out"
    "$T/rust-target-portable/release/examples/bench_corpus" \
        "$T/corpus-manifest.tsv" "$out" 10 1 1024 > "$log" 2>&1
    rc=$?
    rows=$(grep -c '^ROW' "$log")
    printf 'rp\t%s\t%s\t%s\n' "$tag" "$rc" "$rows" >> "$ORDER"
    [ "$rc" -eq 0 ] && [ "$rows" -eq 200 ] || { echo "FAIL rp $tag rc=$rc rows=$rows" >&2; exit 1; }
    echo "OK rp $tag"
}

for p in 1 2 3 4 5 6 7 8; do
    if [ $((p % 2)) -eq 1 ]; then
        run_cp "$p"; sleep 1; run_rp "$p"; sleep 1
    else
        run_rp "$p"; sleep 1; run_cp "$p"; sleep 1
    fi
done
echo PORTABLE_DONE
