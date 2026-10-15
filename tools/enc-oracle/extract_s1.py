#!/usr/bin/env python3
"""Extract committed S1 (M4-S1) Rust testdata from enc_oracle S1 dumps.

Usage: extract_s1.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by enc_oracle_s1, which also contains the
M2/M3 lines parsed by extract_testdata.py) and writes, for each selected
(frame, fixture):

  qc1_<name>_f<F>.txt   S1GEO/SPEC/PENLD/PTHLD/FF/ENFAC/WENLD/WTHLD/SPNL/
                        SPECH/SPCO/SPNA/PECH/ISB/ISS/NOISE per channel,
                        then MSD (stereo), PE, STATIC.

S1 frames were validated for run-to-run stability of every committed line
(3-run comparison; see docs/aac-enc-m4-quant-ratecontrol-plan.md S1
notes). SPEC lines are committed verbatim (1024 ints); positions inside
FF-inactive bands may hold C scratch values on short frames (same pad
story as M3 GSPEC) and are never read by the S1 functions under test.
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# (oracle stem, [frames]). Stereo frames need no per-channel split: the
# qc1 file holds the whole element (both channels + MSD/PE/STATIC).
S1_PLAN = {
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


def parse_s1(path):
    """Parse S1FRAME blocks. Returns {frame: [lines]} (lines after S1FRAME)."""
    frames, cur, in_s1 = {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S1FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S1FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s1 = True
        elif in_s1:
            cur.append(ln)
            if ln.startswith("STATIC "):
                in_s1 = False
                cur = None
    return frames


def check_block(lines, nch):
    """Strict structural check of one S1FRAME block (order + tags).

    Per-channel layout: S1GEO, SOFF, STNSW x nwin, STNSR, then the fixed
    per-channel tail. nwin follows the S1GEO window sequence (2 = short).
    """
    per_ch_tail = ["SPEC", "PENLD", "PTHLD", "SENL", "FF", "ENFAC", "WENLD",
                   "WTHLD", "SPNL", "SPECH", "SPCO", "SPNA", "PECH", "ISB",
                   "ISS", "NOISE"]
    i = 0
    assert lines[i].startswith("ATS pre="), lines[i][:24]
    i += 1
    assert lines[i].startswith("ATS post="), lines[i][:24]
    i += 1
    for c in range(nch):
        assert lines[i].startswith("S1GEO "), lines[i][:24]
        seq = int(lines[i].split("seq=")[1])
        nwin = 8 if seq == 2 else 1
        i += 1
        assert lines[i].startswith("SOFF "), lines[i][:24]
        i += 1
        for _ in range(nwin):
            assert lines[i].startswith("STNSW "), lines[i][:40]
            i += 1
        assert lines[i].startswith("STNSR"), lines[i][:24]
        i += 1
        for tag in per_ch_tail:
            assert lines[i].startswith(tag + " "), \
                f"expected {tag} ch{c}, got {lines[i][:24]}"
            if tag == "SPEC":
                # "SPEC ch=<c>" + 1024 coefficients
                assert len(lines[i].split()) == 1026, \
                    f"SPEC length {len(lines[i].split())}"
            i += 1
    if nch == 2:
        assert lines[i].startswith("MSD "), lines[i][:24]
        i += 1
    assert lines[i].startswith("PE "), lines[i][:24]
    assert lines[i + 1].startswith("STATIC "), lines[i + 1][:24]
    assert i + 2 == len(lines), f"trailing lines: {lines[i+2:]}"


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s1frames in S1_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        frames = parse_s1(path)
        # channel count from the header comment of the dump
        head = open(path).readline()
        nch = int(head.split("ch=")[1].split()[0])
        name = short_name(stem)
        for f in s1frames:
            assert f in frames, f"{stem}: S1FRAME {f} missing"
            check_block(frames[f], nch)
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc1_{name}_f{f}.txt"), "w") as out:
                out.write(f"S1FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s1frames)} S1 frames extracted")
    print(f"total {total} qc1 files")


if __name__ == "__main__":
    main()
