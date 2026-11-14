#!/usr/bin/env python3
"""V4.32 three-way boundary analysis: M3 f64 vs Q31 vs recipe reference."""
import re, struct

TD = "/Users/USER/Documents/VSCode/fdk-aac/fdk-aac/rust/src/aac_enc/testdata"
LOG = "/private/var/folders/*/T/opencode/v4.32-mdctq31-20261020/v432-live.log"
V431 = "/private/var/folders/*/T/opencode/v4.31-detin-20261020/v431-live.log"

m3, q31, old = {}, {}, {}
for l in open(LOG):
    m = re.match(r"V432M3 ch=(\d) M3=\[(.*)\]", l.strip())
    if m:
        c = int(m.group(1))
        m3[c] = [float(x) for x in m.group(2).split(",")]
    m = re.match(r"V432Q ch=(\d) w=(\d) SPEC=\[(.*)\]", l.strip())
    if m:
        q31[(int(m.group(1)), int(m.group(2)))] = list(map(int, m.group(3).split(",")))
for l in open(V431):
    m = re.match(r"V431DET ch=(\d) w=(\d) SPEC=\[(.*)\]", l.strip())
    if m:
        old[(int(m.group(1)), int(m.group(2)))] = list(map(int, m.group(3).split(",")))
print("m3:", sorted(m3), "lens:", sorted(set(len(v) for v in m3.values())))

# Q31 same-run vs V4.31 reuse check
same = all(q31[k] == old[k] for k in q31)
print(f"V432 Q31 == V4.31 Q31 slices: {same}")

def wrap32(v):
    v &= 0xFFFFFFFF
    return v - 0x100000000 if v >= 0x80000000 else v

def mdct(name, f, ch):
    nums = open(f"{TD}/mdct_{name}_f{f}_ch{ch}.txt").read().split()
    return int(nums[0]), list(map(int, nums[1:]))

for ch in [0, 1]:
    e, ints = mdct("onset-stereo", 24, ch)
    assert len(ints) == 1024 and len(m3[ch]) == 1024
    ref_f64 = [v * 2.0 ** (e - 31) for v in ints]
    # (A) live M3 f64 vs reference f64, per window over full 128 lines
    print(f"--- ch{ch} (A) liveM3 vs refF64, per window ---")
    for w in range(8):
        seg_live = m3[ch][w * 128:(w + 1) * 128]
        seg_ref = ref_f64[w * 128:(w + 1) * 128]
        d = [(i, a, b) for i, (a, b) in enumerate(zip(seg_live, seg_ref)) if a != b]
        ma = max([abs(a - b) for _, a, b in d] or [0.0])
        mr = max([abs(a - b) / max(abs(b), 1e-300) for _, a, b in d if b != 0.0] or [0.0])
        sig = max([abs(b) for b in seg_ref] or [0.0])
        print(f"w{w}: diffs {len(d)}/128 maxabs {ma:.6e} maxrel {mr:.3e} refmax {sig:.6e}"
              + (f" first idx{d[0][0]} live {d[0][1]!r} ref {d[0][2]!r}" if d else ""))
    # (B) conversion exactness: liveQ31 vs shift(round(liveM3 * 2^22), F) for integer F
    print(f"--- ch{ch} (B) liveQ31 vs scaled liveM3: ratio constancy ---")
    for w in range(8):
        seg_q = q31[(ch, w)]
        seg_m = m3[ch][w * 128:(w + 1) * 128]
        ratios = {}
        for a, qi in zip(seg_m, seg_q):
            base = round(a * 2.0 ** 22)
            if base != 0 and qi != 0:
                ratios[qi / base] = ratios.get(qi / base, 0) + 1
        top = sorted(ratios.items(), key=lambda kv: -kv[1])[:4]
        print(f"w{w}: distinct ratios {len(ratios)} top {[(r, n) for r, n in top]}")
