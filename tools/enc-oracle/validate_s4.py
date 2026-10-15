#!/usr/bin/env python3
"""Validate run-to-run stability of committed S4 frames across 3 runs.

Usage: validate_s4.py <run0-dir> <run1-dir> <run2-dir>

For every qc4 frame in S4_PLAN (imported from extract_s4's plan): every
S4 output line (S4GEO/S4OFF/S4SCF/S4GAIN/S4Q/S4SPEC/S4DIST/S4EN) must be
byte-identical across runs. S4 inputs are S1 spectra (inactive bands
zeroed) plus S2 thresholds, both proven deterministic by
validate_s1.py/validate_s2.py; any difference here is a hard failure
with no band exemption.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s4 import S4_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s4_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S4FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            out[cur].append(ln)
            if ln.startswith("S4END"):
                cur = None
    return out


fail = 0
for stem, s4frames in S4_PLAN.items():
    runs = [s4_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s4frames:
        blocks = [r[f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S4 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
if fail:
    print(f"S4-UNSTABLE ({fail} frames)")
    sys.exit(1)
print("S4-STABLE")
