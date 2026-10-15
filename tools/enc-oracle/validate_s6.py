#!/usr/bin/env python3
"""Validate run-to-run stability of committed S6 frames across 3 runs.

Usage: validate_s6.py <run0-dir> <run1-dir> <run2-dir>

For every S6 frame in S6_PLAN (imported from extract_s6's plan) plus the
three S6INIT lines: every S6 output line (S6INIT/S6BITR/S6GRANT/S6IT/
S6FINAL) must be byte-identical across runs. S6 consumes deterministic
S4/S5 outputs (S4/S5-STABLE) plus persistent QC/ATS state replayed
identically per run; any difference here is a hard failure.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s6 import S6_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3

INIT_MAP = {
    "qc6init_mono-48k.txt": "m2-click-mono-48k",
    "qc6init_stereo-48k.txt": "m2-sine-stereo-48k",
    "qc6init_stereo-44100.txt": "m3-sine-stereo-441k",
}


def s6_file(path):
    """Returns (init_line, {frame: [block lines]})."""
    init, out, cur = None, {}, None
    for ln in open(path).read().splitlines():
        if ln.startswith("S6INIT "):
            init = ln
        elif ln.startswith("S6FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            out[cur].append(ln)
            if ln.startswith("S6END"):
                cur = None
    return init, out


fail = 0
for stem, s6frames in S6_PLAN.items():
    runs = [s6_file(f"{d}/{stem}.s1.txt") for d in dirs]
    if not (runs[0][0] == runs[1][0] == runs[2][0]):
        print(f"{stem}: S6INIT DIFFERS ACROSS RUNS")
        fail += 1
    for f in s6frames:
        blocks = [r[1][f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S6 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
if fail:
    print(f"S6-UNSTABLE ({fail} frames)")
    sys.exit(1)
print("S6-STABLE")
