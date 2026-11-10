#!/bin/bash
# Full C-vs-Rust measurement series for the 2026-11-10 benchmark.
# 3 standalone C + 3 standalone Rust, 8 alternating pairs, 2 post C + 2 post Rust.
set -u
T=/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10
MANIFEST="$T/corpus-manifest.tsv"
REPS=15
WARMUP=1
ORDER="$T/run-order.tsv"
: > "$ORDER"

run_c() {
    local tag="$1"
    local log="$T/logs/c-$tag.log"
    local out="$T/out/c-$tag"
    rm -rf "$out"; mkdir -p "$out"
    "$T/bin/c_bench_corpus" "$MANIFEST" "$out" "$REPS" "$WARMUP" > "$log" 2>&1
    local rc=$?
    local rows=$(grep -c '^ROW' "$log")
    local warm=$(grep -c '^WARM' "$log")
    local done=$(grep -c '^DONE' "$log")
    printf 'c\t%s\t%s\t%s\t%s\t%s\n' "$tag" "$rc" "$rows" "$warm" "$done" >> "$ORDER"
    if [ "$rc" -ne 0 ] || [ "$rows" -ne 300 ] || [ "$warm" -ne 20 ] || [ "$done" -ne 1 ]; then
        echo "FAIL c $tag rc=$rc rows=$rows warm=$warm done=$done" >&2
        tail -20 "$log" >&2
        exit 1
    fi
    echo "OK c $tag rows=$rows"
}

run_r() {
    local tag="$1"
    local log="$T/logs/r-$tag.log"
    local out="$T/out/r-$tag"
    rm -rf "$out"; mkdir -p "$out"
    "$T/rust-target/release/examples/bench_corpus" "$MANIFEST" "$out" "$REPS" "$WARMUP" 1024 > "$log" 2>&1
    local rc=$?
    local rows=$(grep -c '^ROW' "$log")
    local warm=$(grep -c '^WARM' "$log")
    local done=$(grep -c '^DONE' "$log")
    printf 'r\t%s\t%s\t%s\t%s\t%s\n' "$tag" "$rc" "$rows" "$warm" "$done" >> "$ORDER"
    if [ "$rc" -ne 0 ] || [ "$rows" -ne 300 ] || [ "$warm" -ne 20 ] || [ "$done" -ne 1 ]; then
        echo "FAIL r $tag rc=$rc rows=$rows warm=$warm done=$done" >&2
        tail -20 "$log" >&2
        exit 1
    fi
    echo "OK r $tag rows=$rows"
}

{
    echo "series_start_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "load_before=$(uptime)"
    pmset -g therm 2>/dev/null | sed 's/^/thermal_before: /'
} > "$T/logs/series-environment.log"

for i in 1 2 3; do run_c "stand-$i"; sleep 1; done
for i in 1 2 3; do run_r "stand-$i"; sleep 1; done

for p in 1 2 3 4 5 6 7 8; do
    if [ $((p % 2)) -eq 1 ]; then
        run_c "pair-$p"; sleep 1
        run_r "pair-$p"; sleep 1
    else
        run_r "pair-$p"; sleep 1
        run_c "pair-$p"; sleep 1
    fi
done

run_c "post-1"; sleep 1
run_c "post-2"; sleep 1
run_r "post-1"; sleep 1
run_r "post-2"

{
    echo "series_end_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "load_after=$(uptime)"
    pmset -g therm 2>/dev/null | sed 's/^/thermal_after: /'
} >> "$T/logs/series-environment.log"

echo "SERIES_DONE"
cat "$ORDER"
