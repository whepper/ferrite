#!/usr/bin/env python3
"""Validate run-to-run stability of committed S3 frames across 3 runs.

Usage: validate_s3.py <run0-dir> <run1-dir> <run2-dir>

For every qc3 frame in S3_PLAN (imported from extract_s3's plan): every
S3 output line (S3GEO/S3OFF/S3Q/S3DIST/S3EN) must be byte-identical
across runs. S3 inputs are S1 spectra read inside QuantizeSpectrum's
active bands only, so the M3/S1 SPEC-tail scratch (confined to
FF-inactive bands, proven by validate_s1.py) cannot leak in; any
difference here is therefore a hard failure, with no band exemption.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s3 import S3_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s3_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S3FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            out[cur].append(ln)
            if ln.startswith("S3END"):
                cur = None
    return out


fail = 0
for stem, s3frames in S3_PLAN.items():
    runs = [s3_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s3frames:
        blocks = [r[f] for r in runs]
        if not (blocks[0] == blocks[1] == blocks[2]):
            print(f"{stem} f={f}: S3 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
if fail:
    print(f"S3-UNSTABLE ({fail} frames)")
    sys.exit(1)
print("S3-STABLE")
