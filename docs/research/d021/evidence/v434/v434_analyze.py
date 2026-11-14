#!/usr/bin/env python3
"""V4.34 slope-table + state analysis: live vs C Q31 vs analytic; fold effect."""
import re, struct, math

LOG = "/private/var/folders/*/T/opencode/v4.34-winstate-20261020/v434-live.log"
CPP = "/Users/USER/Documents/VSCode/fdk-aac/fdk-aac/cpp/libFDK/src/FDK_tools_rom.cpp"

entries = []
for l in open(LOG):
    m = re.match(r"V434SLOPE entry=(\d+) wlen=(\d+) plen=(\d+) ptl=(\d+) tl=(\d+) BITS=\[(.*)\]", l.strip())
    if m:
        e = int(m.group(1))
        entries.append((e, int(m.group(2)), int(m.group(3)), int(m.group(4)), int(m.group(5)),
                        list(map(int, m.group(6).split(",")))))
print(f"entries: {len(entries)}")
bits0 = entries[0][5]
print("all wrs bit-identical across 16 entries:", all(e[5] == bits0 for e in entries))
live = [(struct.unpack("<f", struct.pack("<I", b))[0]) for b in bits0]
print(f"wrs len complex: {len(live)//2}")
lre = live[0::2]
lim = live[1::2]

# C SineWindow128
src = open(CPP).read()
i = src.index("const FIXP_WTP SineWindow128[] = {")
j = src.index("};", i)
pairs = re.findall(r"WTCP\((0x[0-9a-f]+),\s*(0x[0-9a-f]+)\)", src[i:j])
print("C SineWindow128 pairs:", len(pairs))
cre = [int(a, 16) / 2.0**31 for a, b in pairs]
cim = [int(b, 16) / 2.0**31 for a, b in pairs]

# live vs C element-wise (live f32 exact via bits)
def f32bits(x):
    return struct.unpack("<f", struct.pack("<f", x))[0]
n = min(len(lre), len(cre))
d = [(k, lre[k], cre[k]) for k in range(n) if lre[k] != f32bits(cre[k])]
print(f"live-vs-C (f32-rounded C as reference): {len(d)}/{n} differ")
if d:
    ma = max(abs(a - b) for _, a, b in d)
    print(f"maxabs {ma:.3e}; first idx{d[0][0]} live {d[0][1]!r} C {d[0][2]!r}")
# raw (unrounded) C vs live
d2 = [(k, lre[k], cre[k]) for k in range(n) if abs(lre[k] - cre[k]) > 0]
ma2 = max(abs(a - b) for _, a, b in d2)
print(f"live-vs-C exact-float: maxabs {ma2:.3e} over {len(d2)} lines")

# analytic fit: theta = atan2(im, re); check uniform spacing + sine match
th = [math.atan2(lim[k], lre[k]) for k in range(len(lre))]
dth = [th[k+1] - th[k] for k in range(len(th) - 1)]
print(f"theta range: [{th[0]:.6f}, {th[-1]:.6f}], spacing min/max: {min(dth):.8f}/{max(dth):.8f}")
# fit: theta_k = t0 + k*dt via endpoints
t0, dt = th[0], (th[-1] - th[0]) / (len(th) - 1)
mre = max(max(abs(math.cos(t0+k*dt)-lre[k]), abs(math.sin(t0+k*dt)-lim[k])) for k in range(len(th)))
kmax = max(range(len(th)), key=lambda k: max(abs(math.cos(t0+k*dt)-lre[k]), abs(math.sin(t0+k*dt)-lim[k])))
print(f"analytic (cos,sin) fit: max dev {mre:.3e} at k={kmax}")
# symmetry: pairs (re[k], im[k]) vs reversed?
print(f"unit check max |re^2+im^2-1|: {max(abs(lre[k]**2+lim[k]**2-1) for k in range(len(th))):.3e}")
