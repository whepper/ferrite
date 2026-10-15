#!/usr/bin/env python3
"""Validate run-to-run stability of committed S8 frames across 3 runs.

Usage: validate_s8.py <run0-dir> <run1-dir> <run2-dir>

For every qc8 frame in S8_PLAN (imported from extract_s8's plan): every
S8 output line (S8SIDE/S8ELEM/S8RAW/S8FINAL) must be byte-identical
across runs. S8 inputs are S1 side info + S5 first-pass outputs + S6
totals (all proven deterministic by validate_s1/s5/s6.py); any
difference here is a hard failure with no exemption. The S1 SPEC wobble
(confined to FF-inactive bands) cannot leak into S8 because inactive
lines are zeroed at the S3/S4 input contract and never read downstream.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s8 import S8_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s8_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S8FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            out[cur].append(ln)
            if ln.startswith("S8END"):
                cur = None
    return out


fail = 0
for stem, s8frames in S8_PLAN.items():
    runs = [s8_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s8frames:
        blocks = [r[f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S8 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
if fail:
    print(f"S8-UNSTABLE ({fail} frames)")
    sys.exit(1)
print("S8-STABLE")
