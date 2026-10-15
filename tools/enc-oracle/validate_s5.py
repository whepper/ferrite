#!/usr/bin/env python3
"""Validate run-to-run stability of committed S5 frames across 3 runs.

Usage: validate_s5.py <run0-dir> <run1-dir> <run2-dir>

For every qc5 frame in S5_PLAN (imported from extract_s5's plan): every
S5 output line (S5GEO/S5OFF/S5QMAX/S5ISB/S5ISS/S5NOISE/S5SECT/S5SEC/
S5BITS) must be byte-identical across runs. S5 inputs are S4 inv=2
outputs (proven deterministic by validate_s4.py); any difference here
is a hard failure with no band exemption.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s5 import S5_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s5_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S5FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            out[cur].append(ln)
            if ln.startswith("S5END"):
                cur = None
    return out


fail = 0
for stem, s5frames in S5_PLAN.items():
    runs = [s5_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s5frames:
        blocks = [r[f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S5 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
if fail:
    print(f"S5-UNSTABLE ({fail} frames)")
    sys.exit(1)
print("S5-STABLE")
