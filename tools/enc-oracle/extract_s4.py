#!/usr/bin/env python3
"""Extract committed S4 (M4-S4) Rust testdata from enc_oracle S4 dumps.

Usage: extract_s4.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by the oracle binary, which contains M2/M3
lines plus S1FRAME, S2FRAME..S2END, S4FRAME..S4END and S3FRAME..S3END
blocks) and writes, for each selected (frame, fixture):

  qc4_<name>_f<F>.txt   S4GEO/S4OFF per channel, then per invQuant level
                        (0 = initial formula, 1 = +improve/single,
                        2 = full afterburner) the S4SCF/S4GAIN/S4Q/
                        S4SPEC/S4DIST/S4EN lines.

Inputs (spectra, energies, thresholds, form factors, geometry) come from
the matching qc1/qc2 files; only S4 outputs live here. Every committed
S4Q vector is scanned for -32768 (the reference guard landmine):
presence aborts extraction.
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# Same frames as S1/S2/S3 (S4 consumes S1 spectra + S2 thresholds).
S4_PLAN = {
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


def parse_s4(path):
    """Parse S4FRAME blocks. Returns {frame: [lines]} (lines after S4FRAME)."""
    frames, cur, in_s4 = {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S4FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S4FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s4 = True
        elif in_s4:
            cur.append(ln)
            if ln.startswith("S4END"):
                in_s4 = False
                cur = None
    return frames


def check_block(lines, nch):
    """Strict structural check of one S4FRAME block (order + tags)."""
    i = 0
    for _ in range(nch):
        assert lines[i].startswith("S4GEO "), lines[i][:16]
        i += 1
        assert lines[i].startswith("S4OFF "), lines[i][:16]
        i += 1
    for inv in (0, 1, 2):
        for _ in range(nch):
            assert lines[i].startswith(f"S4SCF inv={inv} "), lines[i][:20]
            i += 1
            assert lines[i].startswith(f"S4GAIN inv={inv} "), lines[i][:20]
            i += 1
            q = lines[i]
            assert q.startswith(f"S4Q inv={inv} "), q[:20]
            vals = [int(x) for x in q.split()[3:]]
            assert len(vals) == 1024, f"S4Q length {len(vals)}"
            assert -32768 not in vals, f"S4Q holds SHORT_MIN: {q[:20]}"
            i += 1
            assert lines[i].startswith(f"S4SPEC inv={inv} "), lines[i][:20]
            assert len(lines[i].split()[3:]) == 1024
            i += 1
            assert lines[i].startswith(f"S4DIST inv={inv} "), lines[i][:20]
            i += 1
            assert lines[i].startswith(f"S4EN inv={inv} "), lines[i][:20]
            i += 1
    assert lines[i].startswith("S4END"), lines[i][:16]
    assert i + 1 == len(lines), f"trailing lines: {lines[i+1:]}"


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s4frames in S4_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        frames = parse_s4(path)
        head = open(path).readline()
        nch = int(head.split("ch=")[1].split()[0])
        name = short_name(stem)
        for f in s4frames:
            assert f in frames, f"{stem}: S4FRAME {f} missing"
            check_block(frames[f], nch)
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc4_{name}_f{f}.txt"), "w") as out:
                out.write(f"S4FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s4frames)} S4 frames extracted")
    print(f"total {total} qc4 files")


if __name__ == "__main__":
    main()
