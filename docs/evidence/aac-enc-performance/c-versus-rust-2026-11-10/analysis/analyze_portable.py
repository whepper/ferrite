#!/usr/bin/env python3
"""Analyze the portable-build (generic CPU) C-vs-Rust paired runs."""
import csv
import glob
import json
import os
import statistics
from collections import defaultdict

T = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10"
OUT = os.path.join(T, "analysis")
REPS = 10


def median(xs):
    return statistics.median(xs)


def parse_c(path):
    rows = defaultdict(dict)
    cfg = {}
    for line in open(path):
        p = line.rstrip("\n").split("\t")
        if p and p[0] == "ROW" and len(p) >= 9:
            stem, rep = p[1], int(p[2])
            rows[stem][rep] = {"total_ns": int(p[3]) + int(p[4]),
                               "frames": int(p[6]), "bytes": int(p[7]), "hash": p[8]}
        elif p and p[0] == "CFG" and len(p) >= 9:
            cfg[p[1]] = {"sr": int(p[2]), "ch": int(p[3]), "samples": int(p[4])}
    for stem in rows:
        for rep in rows[stem]:
            rows[stem][rep].update(cfg[stem])
    return rows


def parse_r(path):
    rows = defaultdict(dict)
    for line in open(path):
        p = line.rstrip("\n").split("\t")
        if p and p[0] == "ROW" and len(p) >= 11:
            rows[p[1]][int(p[2])] = {
                "sr": int(p[3]), "ch": int(p[4]), "samples": int(p[5]),
                "frames": int(p[6]), "bytes": int(p[7]),
                "total_ns": int(p[8]) + int(p[9]), "hash": p[10],
            }
    return rows


def agg(rows):
    sums = [sum(rows[s][rep]["total_ns"] for s in rows) for rep in range(REPS)]
    return median(sums) / 1e6


tags = [str(i) for i in range(1, 9)]
c = {t: parse_c(os.path.join(T, "logs", f"cp-{t}.log")) for t in tags}
r = {t: parse_r(os.path.join(T, "logs", f"rp-{t}.log")) for t in tags}

pair_rows = []
for i, t in enumerate(tags, 1):
    ca, ra = agg(c[t]), agg(r[t])
    pair_rows.append({"pair": i, "order": "C->R" if i % 2 else "R->C",
                      "c_agg_ms": ca, "r_agg_ms": ra,
                      "ratio": ra / ca, "slowdown_pct": (ra / ca - 1) * 100,
                      "speed_advantage_pct": (ca / ra - 1) * 100})
with open(os.path.join(OUT, "portable-paired-summary.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(pair_rows[0]), delimiter="\t")
    w.writeheader()
    for row in pair_rows:
        w.writerow({k: (f"{v:.6f}" if isinstance(v, float) else v) for k, v in row.items()})

stems = sorted(c[tags[0]])
fixture_rows = []
for stem in stems:
    cv = [c[t][stem][rep]["total_ns"] / 1e6 for t in tags for rep in range(REPS)]
    rv = [r[t][stem][rep]["total_ns"] / 1e6 for t in tags for rep in range(REPS)]
    fixture_rows.append({
        "stem": stem, "c_median_ms": median(cv), "r_median_ms": median(rv),
        "ratio": median(rv) / median(cv),
        "slowdown_pct": (median(rv) / median(cv) - 1) * 100,
        "speed_advantage_pct": (median(cv) / median(rv) - 1) * 100,
        "c_min_ms": min(cv), "c_max_ms": max(cv),
        "r_min_ms": min(rv), "r_max_ms": max(rv),
        "frames": c[tags[0]][stem][0]["frames"],
        "c_bytes": c[tags[0]][stem][0]["bytes"],
        "r_bytes": r[tags[0]][stem][0]["bytes"],
    })
with open(os.path.join(OUT, "portable-per-workload.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(fixture_rows[0]), delimiter="\t")
    w.writeheader()
    for row in fixture_rows:
        w.writerow({k: (f"{v:.6f}" if isinstance(v, float) else v) for k, v in row.items()})

slows = [p["slowdown_pct"] for p in pair_rows]
summary = {
    "c_pair_median_ms": median([p["c_agg_ms"] for p in pair_rows]),
    "r_pair_median_ms": median([p["r_agg_ms"] for p in pair_rows]),
    "ratio": median([p["r_agg_ms"] for p in pair_rows]) / median([p["c_agg_ms"] for p in pair_rows]),
    "slowdown_pct": (median([p["r_agg_ms"] for p in pair_rows]) / median([p["c_agg_ms"] for p in pair_rows]) - 1) * 100,
    "pair_slowdown_median_pct": median(slows),
    "pair_slowdown_min_pct": min(slows),
    "pair_slowdown_max_pct": max(slows),
    "pairs_rust_slower": sum(1 for x in slows if x > 0),
    "pairs": len(pair_rows),
    "reps": REPS,
}
with open(os.path.join(OUT, "portable-summary.json"), "w") as f:
    json.dump(summary, f, indent=2)

print(json.dumps(summary, indent=2))
print("\nPer-workload:")
for row in sorted(fixture_rows, key=lambda x: -x["ratio"]):
    print(f"  {row['stem']:<32} C {row['c_median_ms']:8.3f}  R {row['r_median_ms']:8.3f}  ratio {row['ratio']:.4f}  {row['slowdown_pct']:+.2f}%")
