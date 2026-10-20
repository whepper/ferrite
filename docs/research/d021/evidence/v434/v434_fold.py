#!/usr/bin/env python3
"""V4.34 fold-effect estimate: same ring, live vs C coefs, f64 fold; vs observed M3 delta."""
import re, struct, math

W32 = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.32-mdctq31-20261020/v432-live.log"
W33 = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.33-ringmdct-20261020/v433-live.log"
TD = "/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac/rust/src/aac_enc/testdata"
CPP = "/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac/cpp/libFDK/src/FDK_tools_rom.cpp"

def f32bits(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]

m3 = {}
for l in open(W32):
    m = re.match(r"V432M3 ch=(\d) M3=\[(.*)\]", l.strip())
    if m:
        m3[int(m.group(1))] = [f32bits(float(x)) for x in m.group(2).split(",")]
ring = None
for l in open(W33):
    m = re.match(r"V433RING entry=0 .* RING=\[(.*)\]", l.strip())
    if m:
        ring = [struct.unpack("<f", struct.pack("<I", int(x)))[0] for x in m.group(1).split(",")]
assert ring and len(ring) == 2048 and len(m3[0]) == 1024

# live coefs: reuse V4.34 wrs (re-parse quickly from its log)
W34 = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.34-winstate-20261020/v434-live.log"
live_bits = None
for l in open(W34):
    m = re.match(r"V434SLOPE entry=0 .* BITS=\[(.*)\]", l.strip())
    if m:
        live_bits = list(map(int, m.group(1).split(",")))
        break
lw = [struct.unpack("<f", struct.pack("<I", b))[0] for b in live_bits]
lre, lim = lw[0::2], lw[1::2]
assert len(lre) == 64

# C coefs exact float
src = open(CPP).read()
i = src.index("const FIXP_WTP SineWindow128[] = {")
pairs = re.findall(r"WTCP\((0x[0-9a-f]+),\s*(0x[0-9a-f]+)\)", src[i:src.index("};", i)])
cre = [int(a, 16) / 2.0**31 for a, b in pairs]
cim = [int(b, 16) / 2.0**31 for a, b in pairs]
assert len(cre) == 64

# committed MDCT ref f64
nums = open(f"{TD}/mdct_onset-stereo_f24_ch0.txt").read().split()
e, ints = int(nums[0]), list(map(int, nums[1:]))
ref = [v * 2.0**(e - 31) for v in ints]

# steady-SHORT fold (tl=128, nl=nr=0, fl=fr=128), spectrum s of 8
def fold(spec_ring, wre, wim):
    out = [0.0] * 128
    for k in range(64):
        out[64 + k] = (spec_ring[k] * wim[k] - spec_ring[127 - k] * wre[k]) * 2.0
    for i in range(64):
        j = 63 - i
        out[i] = -((spec_ring[192 + i] * wim[j] + spec_ring[191 - i] * wre[j])) * 2.0
    return out

print("s : max|foldLive-foldC|  max|M3live-M3ref|  (pre-dctiv vs post-dctiv, same bins)")
tot_f = 0.0
tot_o = 0.0
for s in range(8):
    seg = ring[448 + 128 * s:448 + 128 * s + 256]
    fl = fold(seg, lre, lim)
    fc = fold(seg, cre, cim)
    df = [abs(a - b) for a, b in zip(fl, fc)]
    mo = [abs(m3[0][128 * s + k] - ref[128 * s + k]) for k in range(128)]
    print(f"{s} : {max(df):.3e} (bin {max(range(128), key=lambda k: df[k])})"
          f"   {max(mo):.3e} (bin {max(range(128), key=lambda k: mo[k])})")
    tot_f = max(tot_f, max(df))
    tot_o = max(tot_o, max(mo))
print(f"overall: fold-coef effect {tot_f:.3e} vs observed M3 delta {tot_o:.3e}")
