#!/usr/bin/env python3
"""Validate run-to-run stability of committed S2 frames across 3 runs.

Usage: validate_s2.py <run0-dir> <run1-dir> <run2-dir>

For every qc2 frame in S2_PLAN (imported from extract_s2's plan): every
S2 output line must be byte-identical across runs. S2 inputs derive from
S1 vectors plus deterministic config, so unlike SPEC no wobble is
expected anywhere.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s2 import S2_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s2_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S2FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            if ln.startswith("S2END"):
                out[cur].append(ln)
                cur = None
            else:
                out[cur].append(ln)
    return out


fail = 0
for stem, s2frames in S2_PLAN.items():
    runs = [s2_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s2frames:
        blocks = [r[f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S2 OUTPUT LINES DIFFER ACROSS RUNS")
            for i, b in enumerate(blocks):
                for j, other in enumerate(blocks):
                    if i < j and b != other:
                        for k, (a, c) in enumerate(zip(b, other)):
                            if a != c:
                                print(f"  run{i} vs run{j} line{k}:")
                                print(f"    {a[:100]}")
                                print(f"    {c[:100]}")
                                break
            fail += 1
print("S2-STABLE" if fail == 0 else f"S2-UNSTABLE ({fail})")
sys.exit(1 if fail else 0)
