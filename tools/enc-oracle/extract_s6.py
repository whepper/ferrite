#!/usr/bin/env python3
"""Extract committed S6 (M4-S6) Rust testdata from enc_oracle S6 dumps.

Usage: extract_s6.py <oracle-dir> <testdata-dir>

Reads <stem>.s1.txt (produced by the oracle binary: S6INIT once per file
plus S6FRAME..S6END blocks per frame) and writes, for each selected frame:

  qc6_<name>_f<F>.txt   S6FRAME/S6BITR/S6GRANT per frame, then per outer
                        gain-loop pass one S6IT line (strict pass order
                        0..npass-1), then S6FINAL/S6END.

plus one init fixture per (channels, sample-rate) operating point:

  qc6init_mono-48k.txt / qc6init_stereo-48k.txt / qc6init_stereo-44100.txt
                        the S6INIT line (QC init vectors from the real
                        QCInit path).

Inputs (S4/S5 first-pass outputs, S1 PE/static) come from the matching
qc1/qc4/qc5 files; only S6 accounting (grants, iteration trace, fill and
finalization) lives here. S6FINAL `total` is the ADTS-payload bit count:
total+56 (probed ADTS header) equals the real `aac-enc` frame length on
every frame where oracle inputs match the encoder (all long windows; the
known short-block blips are documented in the S6 record, not hidden).
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# S5 corpus (all S1-S5 frames) + F6 stereo stress (loud short+long mix).
S6_PLAN = {
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

# (channels, rate) operating points covered by one init fixture each
# (mapping kept explicit below for auditability).


def short_name(stem):
    return stem.replace("m2-", "").replace("m3-", "").replace("m6-", "") \
               .replace("-48k", "").replace("-441k", "-44100")


def parse_s6(path):
    """Returns (init_line, {frame: [lines after S6FRAME]})."""
    init, frames, cur, in_s6 = None, {}, None, False
    for ln in open(path).read().splitlines():
        if not ln.strip():
            continue
        if ln.startswith("S6INIT "):
            assert init is None, "duplicate S6INIT"
            init = ln
        elif ln.startswith("S6FRAME "):
            f = int(ln.split("=")[1])
            assert f not in frames, f"duplicate S6FRAME {f}"
            frames[f] = []
            cur = frames[f]
            in_s6 = True
        elif in_s6:
            cur.append(ln)
            if ln.startswith("S6END"):
                in_s6 = False
                cur = None
    assert init is not None, f"{path}: S6INIT missing"
    return init, frames


def check_block(lines):
    """Strict structural check of one S6FRAME block."""
    assert lines[0].startswith("S6BITR "), lines[0][:16]
    assert lines[1].startswith("S6GRANT "), lines[1][:16]
    assert lines[2].startswith("S6GSTATE "), lines[2][:16]
    assert len(lines[2].split()) == 7, "S6GSTATE fields"
    i = 3
    passes = 0
    while lines[i].startswith("S6IT "):
        assert f"pass={passes} " in lines[i], f"S6IT order: {lines[i][:24]}"
        passes += 1
        i += 1
    assert lines[i].startswith("S6FINAL "), lines[i][:16]
    npass = int(lines[i].split("npass=")[1].split()[0])
    assert npass == passes, f"npass {npass} != S6IT lines {passes}"
    assert lines[i + 1].startswith("S6GPOST "), lines[i + 1][:16]
    assert len(lines[i + 1].split()) == 7, "S6GPOST fields"
    assert lines[i + 2].startswith("S6END"), lines[i + 2][:16]
    assert i + 3 == len(lines), f"trailing lines: {lines[i+3:]}"


def check_init(line, channels, rate):
    assert line.startswith("S6INIT "), line[:16]
    kv = dict(kv.split("=") for kv in line.split()[1:])
    # VBR-5 invariants ( Categories: buffer, reservoir, distribution ).
    n_eff = channels
    assert int(kv["max"]) == 6144 * n_eff, "maxBits"
    assert int(kv["min"]) == 0, "minBits"
    assert int(kv["bitres"]) == 6144 * n_eff, "bitRes"
    assert int(kv["mode"]) == 0, "bitResMode FULL"
    assert int(kv["distmode"]) == 1, "bitDistributionMode"
    return kv


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    total = 0
    for stem, s6frames in S6_PLAN.items():
        path = os.path.join(ORACLE_DIR, stem + ".s1.txt")
        init, frames = parse_s6(path)
        name = short_name(stem)
        for f in s6frames:
            assert f in frames, f"{stem}: S6FRAME {f} missing"
            check_block(frames[f])
            with open(os.path.join(TESTDATA_DIR,
                                   f"qc6_{name}_f{f}.txt"), "w") as out:
                out.write(f"S6FRAME f={f}\n")
                out.write("\n".join(frames[f]) + "\n")
            total += 1
        print(f"{stem}: {len(s6frames)} S6 frames extracted")
    # Fixed init mapping (kept explicit for auditability).
    init_map = {
        "qc6init_mono-48k.txt": ("m2-click-mono-48k", 1, 48000),
        "qc6init_stereo-48k.txt": ("m2-sine-stereo-48k", 2, 48000),
        "qc6init_stereo-44100.txt": ("m3-sine-stereo-441k", 2, 44100),
    }
    for fname, (stem, ch, rate) in init_map.items():
        init, _ = parse_s6(os.path.join(ORACLE_DIR, stem + ".s1.txt"))
        check_init(init, ch, rate)
        with open(os.path.join(TESTDATA_DIR, fname), "w") as out:
            out.write(init + "\n")
        print(f"{fname}: init from {stem}")
    print(f"total {total} qc6 frame files + 3 init files")


if __name__ == "__main__":
    main()
