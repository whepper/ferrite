#!/usr/bin/env python3
"""Extract committed S3 (M4-S3) Rust testdata from enc_oracle S3 dumps.

Usage: extract_s3.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by the oracle binary, which contains M2/M3
lines plus S1FRAME, S2FRAME..S2END and S3FRAME..S3END blocks) and writes,
for each selected (frame, fixture):

  qc3_<name>_f<F>.txt   S3GEO/S3OFF per channel, then per grid gain the
                        S3Q (full 1024-line quantized spectrum), S3DIST
                        (per-band Ld distortion) and S3EN (per-band
                        energy + distortion with committed lines fed back).

Inputs (spectra, geometry) come from the matching qc1 file; only S3
outputs live here. Gains [-72, -32, -8, 64] span clipping/wrap (22%
nonzero, 3 wrapped vectors at -72), dense (5%), sparse (0.7%) and the
all-zero dead-zone path. Every committed S3Q vector is scanned for
-32768 (the reference guard landmine): presence aborts extraction.
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# Same frames as S1/S2 (S3 consumes S1 spectra).
S3_PLAN = {
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


def parse_s3(path):
    """Parse S3FRAME blocks. Returns {frame: [lines]} (lines after S3FRAME)."""
    frames, cur, in_s3 = {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S3FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S3FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s3 = True
        elif in_s3:
            cur.append(ln)
            if ln.startswith("S3END"):
                in_s3 = False
                cur = None
    return frames


def check_block(lines, nch):
    """Strict structural check of one S3FRAME block (order + tags)."""
    i = 0
    for _ in range(nch):
        assert lines[i].startswith("S3GEO "), lines[i][:16]
        i += 1
        assert lines[i].startswith("S3OFF "), lines[i][:16]
        i += 1
        while i < len(lines) and lines[i].startswith("S3Q "):
            assert lines[i].startswith("S3Q g="), lines[i][:16]
            # landmine scan: -32768 slips the reference |q| guard
            vals = [int(x) for x in lines[i].split()[3:]]
            assert len(vals) == 1024, f"S3Q length {len(vals)}"
            assert -32768 not in vals, f"S3Q holds SHORT_MIN: {lines[i][:16]}"
            i += 1
            assert lines[i].startswith("S3DIST g="), lines[i][:16]
            i += 1
            assert lines[i].startswith("S3EN g="), lines[i][:16]
            i += 1
    assert lines[i].startswith("S3END"), lines[i][:16]
    assert i + 1 == len(lines), f"trailing lines: {lines[i+1:]}"


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s3frames in S3_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        frames = parse_s3(path)
        head = open(path).readline()
        nch = int(head.split("ch=")[1].split()[0])
        name = short_name(stem)
        for f in s3frames:
            assert f in frames, f"{stem}: S3FRAME {f} missing"
            check_block(frames[f], nch)
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc3_{name}_f{f}.txt"), "w") as out:
                out.write(f"S3FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s3frames)} S3 frames extracted")
    print(f"total {total} qc3 files")


if __name__ == "__main__":
    main()
