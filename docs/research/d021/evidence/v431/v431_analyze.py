#!/usr/bin/env python3
"""V4.31 detect-input parity: live slices vs MDCT-reconstruction recipe."""
import re

TD = "/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac/rust/src/aac_enc/testdata"
LOG = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.31-detin-20261020/v431-live.log"

live = {}
for l in open(LOG):
    m = re.match(r"V431DET ch=(\d) w=(\d) SPEC=\[(.*)\]", l.strip())
    if m:
        c, w = int(m.group(1)), int(m.group(2))
        live[(c, w)] = list(map(int, m.group(3).split(",")))
print("live slices:", len(live), "lens:", sorted(set(len(v) for v in live.values())))

def wrap32(v):
    v &= 0xFFFFFFFF
    return v - 0x100000000 if v >= 0x80000000 else v

def mdct(name, f, ch):
    nums = open(f"{TD}/mdct_{name}_f{f}_ch{ch}.txt").read().split()
    return int(nums[0]), list(map(int, nums[1:]))

for ch in [0, 1]:
    e, ints = mdct("onset-stereo", 24, ch)
    assert len(ints) == 1024, len(ints)
    spec = ints[:]
    for w in range(8):
        for i in range(w * 128 + 102, (w + 1) * 128):
            spec[i] = 0
        for i in range(w * 128, w * 128 + 102):
            spec[i] = wrap32(spec[i] << 2)
    tot = 0
    ma = 0
    first = None
    print(f"--- ch{ch} (mdctExp={e}) ---")
    for w in range(8):
        got = live[(ch, w)]
        want = spec[w * 128:(w + 1) * 128]
        d = [(i, a, b) for i, (a, b) in enumerate(zip(got, want)) if a != b]
        tot += len(d)
        if d:
            ma = max(ma, max(abs(a - b) for _, a, b in d))
            if first is None:
                first = (w, d[0])
            print(f"w{w}: diffs {len(d)}/128 first idx{d[0][0]} live {d[0][1]} ref {d[0][2]}")
        else:
            print(f"w{w}: diffs 0/128")
    print(f"ch{ch} total diffs {tot}/1024 maxabs {ma} first {first}")
