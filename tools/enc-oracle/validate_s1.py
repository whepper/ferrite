#!/usr/bin/env python3
"""Validate run-to-run stability of committed S1 frames across 3 runs.

Usage: validate_s1.py <run0-dir> <run1-dir> <run2-dir>

For every qc1 frame in S1_PLAN (imported from extract_s1's plan): all S1
output lines (FF/ENFAC/WENLD/WTHLD/SPNL/SPECH/SPCO/SPNA/PECH/ISB/ISS/
NOISE/MSD/PE/STATIC/S1GEO) must be byte-identical across runs; SPEC
differences, if any, must lie entirely inside FF-inactive bands
(verified per frame against grouped/long offsets + maxSfb from S1GEO).
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from extract_s1 import S1_PLAN  # noqa: E402

dirs = sys.argv[1:4]
assert len(dirs) == 3


def s1_lines(path):
    out = {}
    cur = None
    for ln in open(path).read().splitlines():
        if ln.startswith("S1FRAME "):
            cur = int(ln.split("=")[1])
            out[cur] = []
        elif cur is not None:
            if ln.startswith("STATIC "):
                out[cur].append(ln)
                cur = None
            else:
                out[cur].append(ln)
    return out


def offsets_for(dump_path, stem, f):
    """Grouped offsets (GOFF) of frame f, channel 0 from the M3 dump.

    GOFF lines appear in the per-frame GROUPED section in channel order;
    FRAME lines precede them, so associate by frame with an in-order
    channel counter (same positional convention as extract_testdata.py).
    """
    lines = open(dump_path).read().splitlines()
    goffs, cur_frame, ch_seen = {}, None, 0
    for ln in lines:
        if ln.startswith("FRAME "):
            fr = int(ln.split()[1])
            if fr != cur_frame:
                cur_frame = fr
                ch_seen = 0
        elif ln.startswith("GOFF ") and cur_frame is not None:
            goffs.setdefault(cur_frame, []).append(
                list(map(int, ln.split()[1:])))
            ch_seen += 1
    assert f in goffs and len(goffs[f]) >= 1, f"{stem} f={f}: GOFF missing"
    return goffs[f][0]


fail = 0
for stem, s1frames in S1_PLAN.items():
    runs = [s1_lines(f"{d}/{stem}.s1.txt") for d in dirs]
    for f in s1frames:
        blocks = [r[f] for r in runs]
        # split SPEC from the rest
        rest = [[l for l in b if not l.startswith("SPEC ")] for b in blocks]
        if not (rest[0] == rest[1] == rest[2]):
            print(f"{stem} f={f}: S1 OUTPUT LINES DIFFER ACROSS RUNS")
            fail += 1
            continue
        specs = [[l for l in b if l.startswith("SPEC ")] for b in blocks]
        # geometry from S1GEO
        geos = {}
        for l in blocks[0]:
            if l.startswith("S1GEO "):
                p = dict(kv.split("=") for kv in l.split()[1:])
                geos[int(p["ch"])] = (int(p["cnt"]), int(p["per"]),
                                      int(p["max"]), int(p["seq"]))
        for c, spec_lines in enumerate(zip(*specs)):
            vecs = [list(map(int, s.split()[2:])) for s in spec_lines]
            diff = [j for j in range(1024)
                    if not (vecs[0][j] == vecs[1][j] == vecs[2][j])]
            if not diff:
                continue
            cnt, per, mx, seq = geos[c]
            if seq == 2:  # short: grouped offsets
                off = offsets_for(f"{dirs[0]}/{stem}.s1.txt", stem, f)
                active = set()
                groups = cnt // per
                for g in range(groups):
                    for b in range(mx):
                        active.update(range(off[g * per + b],
                                            off[g * per + b + 1]))
            else:  # long: first mx bands of the long table
                off = None
                # long offsets from cfg file
                import re
                rate = "44100" if stem.endswith("-441k") else "48000"
                pch = "112000" if "-mono" in stem else "96000"
                txt = open(f"rust/src/aac_enc/testdata/cfg_{rate}_{pch}.txt") \
                    .read().splitlines()
                # first OFFSETS line is LONG
                for ln in txt:
                    if ln.startswith("OFFSETS "):
                        off = list(map(int, ln.split()[1:]))
                        break
                active = set(range(0, off[mx]))
            bad = [j for j in diff if j in active]
            tag = f"{stem} f={f} ch={c}"
            if bad:
                print(f"{tag}: UNSTABLE POSITIONS IN ACTIVE BANDS: {bad}")
                fail += 1
            else:
                print(f"{tag}: {len(diff)} unstable positions, all in "
                      f"FF-inactive bands (ok)")
print("S1-STABLE" if fail == 0 else f"S1-UNSTABLE ({fail})")
sys.exit(1 if fail else 0)
