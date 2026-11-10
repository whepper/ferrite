#!/usr/bin/env python3
"""Analyze the 2026-11-10 C-vs-Rust AAC encoder benchmark logs.

Inputs are the raw `ROW` lines from the two corpus harnesses in $T/logs.
Outputs TSV/JSON summaries in $T/analysis and a human-readable summary.
"""
import csv
import glob
import hashlib
import json
import os
import statistics
from collections import defaultdict

T = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10"
OUT = os.path.join(T, "analysis")
os.makedirs(OUT, exist_ok=True)
REPS = 15


def median(xs):
    return statistics.median(xs)


def iqr(xs):
    xs = sorted(xs)
    if len(xs) < 4:
        return 0.0
    q1 = statistics.quantiles(xs, n=4)[0]
    q3 = statistics.quantiles(xs, n=4)[2]
    return q3 - q1


def parse_c(path):
    rows = defaultdict(dict)
    cfg = {}
    with open(path) as f:
        for line in f:
            p = line.rstrip("\n").split("\t")
            if not p:
                continue
            if p[0] == "ROW" and len(p) >= 9:
                stem, rep = p[1], int(p[2])
                init, enc, close = int(p[3]), int(p[4]), int(p[5])
                frames, nbytes, h = int(p[6]), int(p[7]), p[8]
                rows[stem][rep] = {
                    "init_ns": init, "encode_ns": enc, "close_ns": close,
                    "total_ns": init + enc, "frames": frames, "bytes": nbytes,
                    "hash": h,
                }
            elif p[0] == "CFG" and len(p) >= 9:
                cfg[p[1]] = {
                    "sr": int(p[2]), "ch": int(p[3]), "samples": int(p[4]),
                    "frame_length": int(p[5]), "delay": int(p[6]),
                }
    for stem in rows:
        info = cfg[stem]
        for rep in rows[stem]:
            rows[stem][rep].update(info)
    return rows


def parse_r(path):
    rows = defaultdict(dict)
    with open(path) as f:
        for line in f:
            p = line.rstrip("\n").split("\t")
            if p and p[0] == "ROW" and len(p) >= 11:
                stem, rep = p[1], int(p[2])
                rows[stem][rep] = {
                    "sr": int(p[3]), "ch": int(p[4]), "samples": int(p[5]),
                    "frames": int(p[6]), "bytes": int(p[7]),
                    "init_ns": int(p[8]), "encode_ns": int(p[9]),
                    "total_ns": int(p[8]) + int(p[9]), "hash": p[10],
                }
    return rows


def run_aggregate(rows):
    """Median of complete-corpus per-repetition sums (ms)."""
    sums = []
    for rep in range(REPS):
        total = 0
        for stem in rows:
            total += rows[stem][rep]["total_ns"]
        sums.append(total)
    return median(sums) / 1e6, sums


# Load all runs.
runs = {}
for path in sorted(glob.glob(os.path.join(T, "logs", "c-*.log"))):
    tag = os.path.basename(path)[2:-4]
    if "validate" in tag or "smoke" in tag:
        continue
    runs[("c", tag)] = parse_c(path)
for path in sorted(glob.glob(os.path.join(T, "logs", "r-*.log"))):
    tag = os.path.basename(path)[2:-4]
    if "validate" in tag or "smoke" in tag:
        continue
    runs[("r", tag)] = parse_r(path)

tags = sorted({tag for impl, tag in runs})
expected_tags = [f"stand-{i}" for i in (1, 2, 3)] + [f"pair-{i}" for i in range(1, 9)] + ["post-1", "post-2"]
assert sorted(tags) == sorted(expected_tags), tags

# Per-run aggregate table.
run_rows = []
for tag in tags:
    c = runs[("c", tag)]
    r = runs[("r", tag)]
    c_agg, c_sums = run_aggregate(c)
    r_agg, r_sums = run_aggregate(r)
    # Per-run median per-fixture totals.
    c_fix = {s: median([v["total_ns"] for v in c[s].values()]) / 1e6 for s in c}
    r_fix = {s: median([v["total_ns"] for v in r[s].values()]) / 1e6 for s in r}
    run_rows.append({
        "tag": tag,
        "kind": "stand" if tag.startswith("stand") else ("post" if tag.startswith("post") else "pair"),
        "c_agg_ms": c_agg,
        "r_agg_ms": r_agg,
        "ratio_r_over_c": r_agg / c_agg,
        "rust_slowdown_pct": (r_agg / c_agg - 1.0) * 100.0,
        "rust_speed_advantage_pct": (c_agg / r_agg - 1.0) * 100.0,
        "c_min_fixture_ms": min(c_fix.values()),
        "c_max_fixture_ms": max(c_fix.values()),
        "r_min_fixture_ms": min(r_fix.values()),
        "r_max_fixture_ms": max(r_fix.values()),
    })

with open(os.path.join(OUT, "per-run.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(run_rows[0]), delimiter="\t")
    w.writeheader()
    w.writerows(run_rows)

# Paired summary.
pairs = [r for r in run_rows if r["kind"] == "pair"]
with open(os.path.join(OUT, "paired-summary.tsv"), "w", newline="") as f:
    cols = ["pair", "order", "c_agg_ms", "r_agg_ms", "ratio_r_over_c",
            "rust_slowdown_pct", "rust_speed_advantage_pct"]
    w = csv.DictWriter(f, fieldnames=cols, delimiter="\t")
    w.writeheader()
    for i, r in enumerate(pairs, 1):
        order = "C->R" if i % 2 == 1 else "R->C"
        w.writerow({
            "pair": i, "order": order,
            "c_agg_ms": f"{r['c_agg_ms']:.6f}",
            "r_agg_ms": f"{r['r_agg_ms']:.6f}",
            "ratio_r_over_c": f"{r['ratio_r_over_c']:.6f}",
            "rust_slowdown_pct": f"{r['rust_slowdown_pct']:+.4f}",
            "rust_speed_advantage_pct": f"{r['rust_speed_advantage_pct']:+.4f}",
        })

# Per-fixture statistics from paired runs (8 x 15 = 120 samples per impl).
pair_tags = [f"pair-{i}" for i in range(1, 9)]
stems = sorted({s for tag in pair_tags for s in runs[("c", tag)]})
fixture_rows = []
for stem in stems:
    cvals = [runs[("c", tag)][stem][rep]["total_ns"] / 1e6 for tag in pair_tags for rep in range(REPS)]
    rvals = [runs[("r", tag)][stem][rep]["total_ns"] / 1e6 for tag in pair_tags for rep in range(REPS)]
    c = runs[("c", pair_tags[0])][stem][0]
    r = runs[("r", pair_tags[0])][stem][0]
    assert c["frames"] == r["frames"], (stem, c["frames"], r["frames"])
    audio_s = c["samples"] / c["sr"]
    fixture_rows.append({
        "stem": stem,
        "category": "",
        "sr": c["sr"], "ch": c["ch"], "samples": c["samples"],
        "audio_s": audio_s, "frames": c["frames"],
        "c_bytes": c["bytes"], "r_bytes": r["bytes"],
        "c_bitrate_kbps": c["bytes"] * 8 / audio_s / 1000,
        "r_bitrate_kbps": r["bytes"] * 8 / audio_s / 1000,
        "c_median_ms": median(cvals), "r_median_ms": median(rvals),
        "ratio_r_over_c": median(rvals) / median(cvals),
        "rust_slowdown_pct": (median(rvals) / median(cvals) - 1) * 100,
        "rust_speed_advantage_pct": (median(cvals) / median(rvals) - 1) * 100,
        "c_min_ms": min(cvals), "c_max_ms": max(cvals), "c_iqr_ms": iqr(cvals),
        "r_min_ms": min(rvals), "r_max_ms": max(rvals), "r_iqr_ms": iqr(rvals),
        "c_hash": c["hash"], "r_hash": r["hash"],
    })

category_map = {
    "m2-click-mono-48k": "short_diagnostic",
    "m2-edge-mono-48k": "short_diagnostic",
    "m2-impulse-mono-48k": "short_diagnostic",
    "m2-onset-stereo-48k": "short_diagnostic",
    "m2-silence-stereo-48k": "short_diagnostic",
    "m2-sine-stereo-48k": "short_diagnostic",
    "m2-stepdown-stereo-48k": "short_diagnostic",
    "m3-multitone-stereo-48k": "tonal_noise",
    "m3-noise-mono-48k": "tonal_noise",
    "m3-quiet-sine-stereo-48k": "tonal_noise",
    "m3-sine-stereo-441k": "tonal_noise",
    "m3-sweep-stereo-48k": "tonal_noise",
    "m3-tonal-noise-stereo-48k": "tonal_noise",
    "m3-onset-stereo-441k": "transient",
    "m6-stress-stereo-48k": "transient",
    "m7-music-stereo-48k": "music",
    "m8-music-eroica-stereo-48k": "music",
    "m7-speech-clean-stereo-48k": "speech",
    "m7-speech-pauses-stereo-48k": "speech",
    "m7-music-stereo-48k-x4": "long",
}
for row in fixture_rows:
    row["category"] = category_map[row["stem"]]

with open(os.path.join(OUT, "per-workload.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(fixture_rows[0]), delimiter="\t")
    w.writeheader()
    for row in fixture_rows:
        w.writerow({k: (f"{v:.6f}" if isinstance(v, float) else v) for k, v in row.items()})

# Category summaries: per-rep category sums, then median across pair runs.
categories = ["short_diagnostic", "tonal_noise", "transient", "music", "speech", "long"]
cat_rows = []
for cat in categories:
    members = [r["stem"] for r in fixture_rows if r["category"] == cat]
    c_aggs, r_aggs = [], []
    for tag in pair_tags:
        c_agg, _ = run_aggregate({s: runs[("c", tag)][s] for s in members})
        r_agg, _ = run_aggregate({s: runs[("r", tag)][s] for s in members})
        c_aggs.append(c_agg)
        r_aggs.append(r_agg)
    c_med, r_med = median(c_aggs), median(r_aggs)
    cat_rows.append({
        "category": cat, "fixtures": len(members),
        "c_median_ms": c_med, "r_median_ms": r_med,
        "ratio_r_over_c": r_med / c_med,
        "rust_slowdown_pct": (r_med / c_med - 1) * 100,
        "rust_speed_advantage_pct": (c_med / r_med - 1) * 100,
        "c_share_pct": 0.0, "r_share_pct": 0.0,
    })
c_total = sum(r["c_median_ms"] for r in cat_rows)
r_total = sum(r["r_median_ms"] for r in cat_rows)
for r in cat_rows:
    r["c_share_pct"] = r["c_median_ms"] / c_total * 100
    r["r_share_pct"] = r["r_median_ms"] / r_total * 100

with open(os.path.join(OUT, "category-summary.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(cat_rows[0]), delimiter="\t")
    w.writeheader()
    for row in cat_rows:
        w.writerow({k: (f"{v:.6f}" if isinstance(v, float) else v) for k, v in row.items()})

# Aggregate headline.
audio_s = sum(r["audio_s"] for r in fixture_rows)
c_pair_med = median([r["c_agg_ms"] for r in pairs])
r_pair_med = median([r["r_agg_ms"] for r in pairs])
pair_slow = [r["rust_slowdown_pct"] for r in pairs]
pair_speed = [r["rust_speed_advantage_pct"] for r in pairs]
stand = [r for r in run_rows if r["kind"] in ("stand", "post")]
c_stand = median([r["c_agg_ms"] for r in stand])
r_stand = median([r["r_agg_ms"] for r in stand])

summary = {
    "audio_seconds": audio_s,
    "fixtures": len(fixture_rows),
    "reps_per_fixture": REPS,
    "pair_runs": len(pairs),
    "c_pair_median_ms": c_pair_med,
    "r_pair_median_ms": r_pair_med,
    "ratio_r_over_c": r_pair_med / c_pair_med,
    "rust_slowdown_pct": (r_pair_med / c_pair_med - 1) * 100,
    "rust_speed_advantage_pct": (c_pair_med / r_pair_med - 1) * 100,
    "pair_slowdown_median_pct": median(pair_slow),
    "pair_slowdown_min_pct": min(pair_slow),
    "pair_slowdown_max_pct": max(pair_slow),
    "pair_slowdown_iqr_pct": iqr(pair_slow),
    "pair_speed_advantage_median_pct": median(pair_speed),
    "pair_speed_advantage_min_pct": min(pair_speed),
    "pair_speed_advantage_max_pct": max(pair_speed),
    "pairs_rust_slower": sum(1 for x in pair_slow if x > 0),
    "c_stand_median_ms": c_stand,
    "r_stand_median_ms": r_stand,
    "c_realtime_x": audio_s / (c_pair_med / 1e3),
    "r_realtime_x": audio_s / (r_pair_med / 1e3),
    "c_rtf": (c_pair_med / 1e3) / audio_s,
    "r_rtf": (r_pair_med / 1e3) / audio_s,
    "c_post_median_ms": median([r["c_agg_ms"] for r in run_rows if r["kind"] == "post"]),
    "r_post_median_ms": median([r["r_agg_ms"] for r in run_rows if r["kind"] == "post"]),
}
with open(os.path.join(OUT, "summary.json"), "w") as f:
    json.dump(summary, f, indent=2)

# Determinism: each fixture must have one hash across all reps/runs.
determinism = {}
for impl in ("c", "r"):
    for stem in stems:
        hashes = {v["hash"] for tag in tags for v in runs[(impl, tag)][stem].values()}
        determinism[f"{impl}:{stem}"] = sorted(hashes)
bad = {k: v for k, v in determinism.items() if len(v) != 1}
with open(os.path.join(OUT, "determinism.json"), "w") as f:
    json.dump({"bad": bad, "entries": determinism}, f, indent=2)

# Output validation table from the final post runs.
ref_dir = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/f64-mdct-experiment/c-adts"
out_rows = []
for row in fixture_rows:
    stem = row["stem"]
    cpath = os.path.join(T, "out", "c-post-1", stem + ".c.aac")
    rpath = os.path.join(T, "out", "r-post-1", stem + ".rust.aac")
    with open(cpath, "rb") as f:
        cb = f.read()
    with open(rpath, "rb") as f:
        rb = f.read()
    ref = os.path.join(ref_dir, stem + ".aac")
    ref_match = ""
    if os.path.exists(ref):
        with open(ref, "rb") as f:
            ref_match = "yes" if f.read() == cb else "no"
    out_rows.append({
        "stem": stem,
        "c_bytes": len(cb), "r_bytes": len(rb),
        "c_sha256": hashlib.sha256(cb).hexdigest(),
        "r_sha256": hashlib.sha256(rb).hexdigest(),
        "c_fnv1a64": row["c_hash"], "r_fnv1a64": row["r_hash"],
        "hashes_equal": "yes" if cb == rb else "no",
        "c_matches_prior_reference": ref_match,
        "frames": row["frames"],
        "duration_s": row["frames"] * 1024 / row["sr"],
        "c_bitrate_kbps": len(cb) * 8 / (row["frames"] * 1024 / row["sr"]) / 1000,
        "r_bitrate_kbps": len(rb) * 8 / (row["frames"] * 1024 / row["sr"]) / 1000,
    })
with open(os.path.join(OUT, "output-validation.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(out_rows[0]), delimiter="\t")
    w.writeheader()
    w.writerows(out_rows)

# Markdown summary for inspection.
lines = []
lines.append("# C vs Rust benchmark summary (2026-11-10)\n")
lines.append(f"- Corpus: {len(fixture_rows)} workloads, {audio_s:.3f} s audio, {REPS} reps/fixture, {len(pairs)} paired runs")
lines.append(f"- C paired aggregate median: **{c_pair_med:.3f} ms** ({summary['c_realtime_x']:.1f}x realtime, RTF {summary['c_rtf']:.6f})")
lines.append(f"- Rust paired aggregate median: **{r_pair_med:.3f} ms** ({summary['r_realtime_x']:.1f}x realtime, RTF {summary['r_rtf']:.6f})")
lines.append(f"- Ratio Rust/C: **{summary['ratio_r_over_c']:.4f}**")
lines.append(f"- Rust slowdown: **{summary['rust_slowdown_pct']:+.2f}%**; Rust speed advantage: **{summary['rust_speed_advantage_pct']:+.2f}%**")
lines.append(f"- Paired slowdown median {summary['pair_slowdown_median_pct']:+.2f}% (range {summary['pair_slowdown_min_pct']:+.2f}% to {summary['pair_slowdown_max_pct']:+.2f}%, IQR {summary['pair_slowdown_iqr_pct']:.2f} pp); Rust slower in {summary['pairs_rust_slower']}/{len(pairs)} pairs")
lines.append(f"- Standalone/post medians: C {c_stand:.3f} ms, Rust {r_stand:.3f} ms")
lines.append("")
lines.append("## Per-category\n")
lines.append("| Category | Fixtures | C ms | Rust ms | Ratio | Rust slowdown | C share | Rust share |")
lines.append("|---|---:|---:|---:|---:|---:|---:|---:|")
for r in cat_rows:
    lines.append(f"| {r['category']} | {r['fixtures']} | {r['c_median_ms']:.3f} | {r['r_median_ms']:.3f} | {r['ratio_r_over_c']:.4f} | {r['rust_slowdown_pct']:+.2f}% | {r['c_share_pct']:.1f}% | {r['r_share_pct']:.1f}% |")
lines.append("")
lines.append("## Per-workload\n")
lines.append("| Workload | C ms | Rust ms | Ratio | Rust slowdown |")
lines.append("|---|---:|---:|---:|---:|")
for r in sorted(fixture_rows, key=lambda x: -x["ratio_r_over_c"]):
    lines.append(f"| {r['stem']} | {r['c_median_ms']:.3f} | {r['r_median_ms']:.3f} | {r['ratio_r_over_c']:.4f} | {r['rust_slowdown_pct']:+.2f}% |")
with open(os.path.join(OUT, "summary.md"), "w") as f:
    f.write("\n".join(lines) + "\n")

print("\n".join(lines))
print("\nDeterminism failures:", bad if bad else "none")
