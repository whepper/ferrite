#!/usr/bin/env python3
"""Extract committed S5 (M4-S5) Rust testdata from enc_oracle S5 dumps.

Usage: extract_s5.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by the oracle binary, which contains M2/M3
lines plus S1FRAME, S2FRAME..S2END, S4FRAME..S4END, S5FRAME..S5END and
S3FRAME..S3END blocks) and writes, for each selected (frame, fixture):

  qc5_<name>_f<F>.txt   S5GEO/S5OFF per channel, then per channel the
                        S5QMAX/S5ISB/S5ISS/S5NOISE lines plus the
                        S5SECT/S5SEC*/S5BITS sectioning result.

Inputs (quantized lines, scalefactors, geometry) come from the matching
qc4 (inv=2) and qc1 files; only S5 outputs (QMAX + sectioning + counts)
live here. QMAX is the S6-owned `calcMaxValueInSfb` output consumed as
an S5 input; every value is range-checked (0..32768, and the S4-proven
absence of -32768 in the lines is re-asserted via the QMAX bound).
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# Same frames as S1/S2/S3/S4 (S5 consumes S4 inv=2 outputs).
S5_PLAN = {
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


def parse_s5(path):
    """Parse S5FRAME blocks. Returns {frame: [lines]} (lines after S5FRAME)."""
    frames, cur, in_s5 = {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S5FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S5FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s5 = True
        elif in_s5:
            cur.append(ln)
            if ln.startswith("S5END"):
                in_s5 = False
                cur = None
    return frames


def check_block(lines, nch):
    """Strict structural check of one S5FRAME block (order + tags)."""
    i = 0
    for _ in range(nch):
        assert lines[i].startswith("S5GEO "), lines[i][:16]
        i += 1
        assert lines[i].startswith("S5OFF "), lines[i][:16]
        i += 1
    for _ in range(nch):
        assert lines[i].startswith("S5QMAX "), lines[i][:16]
        for v in lines[i].split()[2:]:
            assert 0 <= int(v) <= 32768, f"S5QMAX range: {v}"
        i += 1
        assert lines[i].startswith("S5ISB "), lines[i][:16]
        i += 1
        assert lines[i].startswith("S5ISS "), lines[i][:16]
        i += 1
        assert lines[i].startswith("S5NOISE "), lines[i][:16]
        i += 1
        assert lines[i].startswith("S5SECT "), lines[i][:16]
        nsec = int(lines[i].split("nsec=")[1])
        i += 1
        for s in range(nsec):
            assert lines[i].startswith("S5SEC "), lines[i][:16]
            assert f"i={s} " in lines[i], f"S5SEC order: {lines[i][:24]}"
            i += 1
        assert lines[i].startswith("S5BITS "), lines[i][:16]
        i += 1
    assert lines[i].startswith("S5END"), lines[i][:16]
    assert i + 1 == len(lines), f"trailing lines: {lines[i+1:]}"


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s5frames in S5_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        frames = parse_s5(path)
        head = open(path).readline()
        nch = int(head.split("ch=")[1].split()[0])
        name = short_name(stem)
        for f in s5frames:
            assert f in frames, f"{stem}: S5FRAME {f} missing"
            check_block(frames[f], nch)
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc5_{name}_f{f}.txt"), "w") as out:
                out.write(f"S5FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s5frames)} S5 frames extracted")
    print(f"total {total} qc5 files")


if __name__ == "__main__":
    main()
