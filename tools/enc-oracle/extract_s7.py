#!/usr/bin/env python3
"""Extract committed S7 (M4-S7) Rust testdata from enc_oracle S7 dumps.

Usage: extract_s7.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by the oracle binary, which contains M2/M3
lines plus S1FRAME, S2FRAME..S2END, S3FRAME..S3END, S4FRAME..S4END,
S5FRAME..S5END, S6FRAME..S6END and S7FRAME..S7END blocks) and writes,
for each selected (frame, fixture):

  qc7_<name>_f<F>.txt   S7FRAME head, then per channel the S7SPEC/S7SCF/
                        S7SECH payload lines (emitted bits + MSB-first
                        hex from the real encode functions), then S7END.

Inputs (quantized lines, scalefactors, sections, geometry) come from the
matching qc4 (inv=2) and qc5 files; only S7 outputs (emitted payload bit
strings) live here. Every emitted payload is self-checked by the oracle
against the S5 counts (production WRITE_SEC/SCAL/SPEC agreement); the
extractor re-asserts the bit/hex length agreement per line.
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# S6 corpus: all S1-S5 frames + F6 stereo stress.
S7_PLAN = {
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
    "m6-stress-stereo-48k": [5],
}


def short_name(stem):
    return stem.replace("m2-", "").replace("m3-", "").replace("m6-", "").replace("-48k", "") \
               .replace("-441k", "-44100")


def parse_s7(path):
    """Parse S7FRAME blocks. Returns {frame: [lines]} (lines after S7FRAME)."""
    frames, cur, in_s7 = {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S7FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S7FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s7 = True
        elif in_s7:
            cur.append(ln)
            if ln.startswith("S7END"):
                in_s7 = False
                cur = None
    return frames


def check_block(lines, nch):
    """Strict structural check of one S7FRAME block (order + tags)."""
    i = 0
    for c in range(nch):
        for tag in ("S7SPEC", "S7SCF", "S7SECH"):
            assert lines[i].startswith(f"{tag} "), lines[i][:24]
            assert f"ch={c} " in lines[i], f"{tag} order: {lines[i][:24]}"
            toks = dict(kv.split("=") for kv in lines[i].split()[1:3])
            bits = int(toks["bits"])
            hexs = lines[i].split("hex=")[1] if "hex=" in lines[i] else ""
            assert len(hexs) == ((bits + 7) // 8) * 2, \
                f"{tag} hex length: {lines[i][:40]}"
            i += 1
    assert lines[i].startswith("S7END"), lines[i][:16]
    assert i + 1 == len(lines), f"trailing lines: {lines[i+1:]}"


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s7frames in S7_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        frames = parse_s7(path)
        head = open(path).readline()
        nch = int(head.split("ch=")[1].split()[0])
        name = short_name(stem)
        for f in s7frames:
            assert f in frames, f"{stem}: S7FRAME {f} missing"
            check_block(frames[f], nch)
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc7_{name}_f{f}.txt"), "w") as out:
                out.write(f"S7FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s7frames)} S7 frames extracted")
    print(f"total {total} qc7 files")


if __name__ == "__main__":
    main()
