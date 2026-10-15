#!/usr/bin/env python3
"""Extract committed S2 (M4-S2) Rust testdata from enc_oracle S2 dumps.

Usage: extract_s2.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by the oracle binary, which contains M2/M3
lines plus S1FRAME..STATIC and S2FRAME..S2END blocks) and writes, for each
selected (frame, fixture):

  qc2_<name>_f<F>.txt   S2CHAOS + per-channel S2GEO/GROUPLEN/THREXP/
                        MINSNR0/SPREAD0/THRV/MINSNR1/SPREAD1.

S2 frames were validated for run-to-run stability of every committed line
(3-run comparison via validate_s2.py). Inputs (weighted energies/
thresholds, form factors, EnFac, linear energies) come from the matching
qc1 file; only S2-stage inputs (minSnr/spread init) and outputs live here.
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# (oracle stem, [frames]). Same 16 S1 frames plus onset f23/f25 (closing a
# consecutive f22-f26 chaos-trajectory run) plus quiet-sine f5 (low-level
# non-silent long frame).
S2_PLAN = {
    "m2-silence-stereo-48k": [5],
    "m2-sine-stereo-48k": [1, 5],
    "m2-onset-stereo-48k": [22, 23, 24, 25, 26],
    "m2-stepdown-stereo-48k": [2],
    "m2-click-mono-48k": [11],
    "m2-impulse-mono-48k": [5],
    "m2-edge-mono-48k": [4],
    "m3-multitone-stereo-48k": [1],
    "m3-sweep-stereo-48k": [10],
    "m3-noise-mono-48k": [5],
    "m3-tonal-noise-stereo-48k": [5],
    "m3-quiet-sine-stereo-48k": [5],
    "m3-sine-stereo-441k": [5],
    "m3-onset-stereo-441k": [22],
}


def short_name(stem):
    return stem.replace("m2-", "").replace("m3-", "").replace("m6-", "").replace("-48k", "") \
               .replace("-441k", "-44100")


def parse_s2(path):
    """Parse S2FRAME blocks. Returns {frame: [lines]} (lines after S2FRAME)."""
    frames, cur, in_s2 = {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S2FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S2FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s2 = True
        elif in_s2:
            cur.append(ln)
            if ln.startswith("S2END"):
                in_s2 = False
                cur = None
    return frames


def check_block(lines, nch):
    """Strict structural check of one S2FRAME block (order + tags)."""
    per_ch = ["S2GEO", "GROUPLEN", "THREXP", "MINSNR0", "SPREAD0", "THRV",
              "MINSNR1", "SPREAD1"]
    i = 0
    assert lines[i].startswith("S2CHAOS "), lines[i][:24]
    i += 1
    for c in range(nch):
        for tag in per_ch:
            assert lines[i].startswith(tag + " "), \
                f"expected {tag} ch{c}, got {lines[i][:24]}"
            i += 1
    assert lines[i].startswith("S2END"), lines[i][:24]
    assert i + 1 == len(lines), f"trailing lines: {lines[i+1:]}"


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s2frames in S2_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        frames = parse_s2(path)
        head = open(path).readline()
        nch = int(head.split("ch=")[1].split()[0])
        name = short_name(stem)
        for f in s2frames:
            assert f in frames, f"{stem}: S2FRAME {f} missing"
            check_block(frames[f], nch)
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc2_{name}_f{f}.txt"), "w") as out:
                out.write(f"S2FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s2frames)} S2 frames extracted")
    print(f"total {total} qc2 files")


if __name__ == "__main__":
    main()
