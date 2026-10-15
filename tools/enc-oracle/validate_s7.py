#!/usr/bin/env python3
"""Validate run-to-run stability of committed S7 frames across 3 runs.

Usage: validate_s7.py <run0-dir> <run1-dir> <run2-dir>

For every qc7 frame in S7_PLAN (imported from extract_s7's plan): every
S7 output line (S7SPEC/S7SCF/S7SECH) must be byte-identical across runs.
S7 inputs are S5 first-pass outputs (proven deterministic by
validate_s5.py); any difference here is a hard failure with no band
exemption.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s7 import S7_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s7_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S7FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            out[cur].append(ln)
            if ln.startswith("S7END"):
                cur = None
    return out


fail = 0
for stem, s7frames in S7_PLAN.items():
    runs = [s7_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s7frames:
        blocks = [r[f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S7 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
if fail:
    print(f"S7-UNSTABLE ({fail} frames)")
    sys.exit(1)
print("S7-STABLE")
