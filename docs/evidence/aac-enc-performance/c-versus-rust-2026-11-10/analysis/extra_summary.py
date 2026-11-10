#!/usr/bin/env python3
"""Extra derived summaries and markdown tables for the report."""
import csv
import json
import os
import random
import statistics

T = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10"
A = os.path.join(T, "analysis")

pairs = list(csv.DictReader(open(os.path.join(A, "paired-summary.tsv")), delimiter="\t"))
slow = [float(p["rust_slowdown_pct"]) for p in pairs]
speed = [float(p["rust_speed_advantage_pct"]) for p in pairs]
print("pair_slowdown_median", statistics.median(slow))
print("pair_slowdown_mean", statistics.mean(slow))
print("pair_slowdown_stdev", statistics.stdev(slow))
print("pair_speed_median", statistics.median(speed))
for order in ("C->R", "R->C"):
    vals = [float(p["rust_slowdown_pct"]) for p in pairs if p["order"] == order]
    print("order", order, "n", len(vals), "median", statistics.median(vals), "min", min(vals), "max", max(vals))
random.seed(1)
bs = sorted(statistics.median([random.choice(slow) for _ in slow]) for _ in range(20000))
print("bootstrap_ci", bs[500], bs[19500])

cats = list(csv.DictReader(open(os.path.join(A, "category-summary.tsv")), delimiter="\t"))
csum = sum(float(x["c_median_ms"]) for x in cats)
rsum = sum(float(x["r_median_ms"]) for x in cats)
print("category_total", csum, rsum, (rsum / csum - 1) * 100)
for x in cats:
    c = float(x["c_median_ms"])
    r = float(x["r_median_ms"])
    print("contrib", x["category"], f"{c / csum * 100:.2f}", f"{(r - c) / csum * 100:+.2f}")

rows = list(csv.DictReader(open(os.path.join(A, "per-workload.tsv")), delimiter="\t"))
print("\nPER_WORKLOAD_TABLE")
print("| Workload | Category | C median ms | Rust median ms | Ratio R/C | Rust slowdown | C IQR ms | Rust IQR ms | C / Rust bytes |")
print("|---|---|---:|---:|---:|---:|---:|---:|---:|")
for r in rows:
    print(f"| `{r['stem']}` | {r['category']} | {float(r['c_median_ms']):.3f} | {float(r['r_median_ms']):.3f} | {float(r['ratio_r_over_c']):.4f} | {float(r['rust_slowdown_pct']):+.2f}% | {float(r['c_iqr_ms']):.3f} | {float(r['r_iqr_ms']):.3f} | {r['c_bytes']} / {r['r_bytes']} |")

print("\nCATEGORY_TABLE")
print("| Category | Fixtures | C ms | Rust ms | Ratio R/C | Rust slowdown | C share | Rust share |")
print("|---|---:|---:|---:|---:|---:|---:|---:|")
for r in cats:
    print(f"| {r['category'].replace('_', ' ')} | {r['fixtures']} | {float(r['c_median_ms']):.3f} | {float(r['r_median_ms']):.3f} | {float(r['ratio_r_over_c']):.4f} | {float(r['rust_slowdown_pct']):+.2f}% | {float(r['c_share_pct']):.1f}% | {float(r['r_share_pct']):.1f}% |")

print("\nPROFILE_TABLE")
prof = list(csv.DictReader(open(os.path.join(A, "profile-stages.tsv")), delimiter="\t"))
stages = ["s4_scf_quant", "frontend_psy", "bit_path", "memory_feeder", "rate_control", "driver_api", "other"]
print("| Stage | C music | Rust music | C speech | Rust speech |")
print("|---|---:|---:|---:|---:|---:|")
for s in stages:
    vals = {(r["impl"], r["workload"]): float(r["pct_root"]) for r in prof if r["stage"] == s}
    print(f"| {s} | {vals.get(('c', 'music'), 0):.1f}% | {vals.get(('r', 'music'), 0):.1f}% | {vals.get(('c', 'speech'), 0):.1f}% | {vals.get(('r', 'speech'), 0):.1f}% |")

print("\nPORTABLE_TABLE")
pp = list(csv.DictReader(open(os.path.join(A, "portable-paired-summary.tsv")), delimiter="\t"))
ps = [float(p["slowdown_pct"]) for p in pp]
print("portable_median", statistics.median(ps), "min", min(ps), "max", max(ps), "mean", statistics.mean(ps))
print("\nPORTABLE_WORKLOAD")
pw = list(csv.DictReader(open(os.path.join(A, "portable-per-workload.tsv")), delimiter="\t"))
for r in pw:
    print(f"| `{r['stem']}` | {float(r['c_median_ms']):.3f} | {float(r['r_median_ms']):.3f} | {float(r['ratio']):.4f} | {float(r['slowdown_pct']):+.2f}% |")
