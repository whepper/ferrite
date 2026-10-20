#!/usr/bin/env python3
"""V4.35 exact-arithmetic offline MDCT (one spectrum): fold f64 + naive DCT-IV f64."""
import re, struct, math

W32 = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.32-mdctq31-20261020/v432-live.log"
W33 = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.33-ringmdct-20261020/v433-live.log"
TD = "/Users/jeroen/Documents/VSCode/fdk-aac/fdk-aac/rust/src/aac_enc/testdata"

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
nums = open(f"{TD}/mdct_onset-stereo_f24_ch0.txt").read().split()
e, ints = int(nums[0]), list(map(int, nums[1:]))
ref = [v * 2.0**(e - 31) for v in ints]

# exact-sine coefs from fitted uniform angles (V4.34: t0 + k*dt on unit circle)
W34 = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.34-winstate-20261020/v434-live.log"
for l in open(W34):
    m = re.match(r"V434SLOPE entry=0 .* BITS=\[(.*)\]", l.strip())
    if m:
        b = list(map(int, m.group(1).split(",")))
        lw = [struct.unpack("<f", struct.pack("<I", x))[0] for x in b]
        break
lre, lim = lw[0::2], lw[1::2]
th = [math.atan2(lim[k], lre[k]) for k in range(64)]
t0, dt = th[0], (th[-1] - th[0]) / 63
ere = [math.cos(t0 + k * dt) for k in range(64)]
eim = [math.sin(t0 + k * dt) for k in range(64)]

def fold(seg, wre, wim):
    out = [0.0] * 128
    for k in range(64):
        out[64 + k] = (seg[k] * wim[k] - seg[127 - k] * wre[k]) * 2.0
    for i in range(64):
        j = 63 - i
        out[i] = -((seg[192 + i] * wim[j] + seg[191 - i] * wre[j])) * 2.0
    return out

def dctiv_naive(x):
    n = len(x)
    return [sum(x[m] * math.cos(math.pi / n * (m + 0.5) * (k + 0.5)) for m in range(n))
            for k in range(n)]

S = 4  # active window with big observed delta (bin 114)
seg = ring[448 + 128 * S:448 + 128 * S + 256]
fx = fold(seg, ere, eim)
ex = dctiv_naive(fx)
lv = m3[0][128 * S:128 * S + 128]
rf = ref[128 * S:128 * S + 128]
dl = [abs(a - b) for a, b in zip(ex, lv)]
dc = [abs(a - b) for a, b in zip(ex, rf)]
print(f"spectrum {S}: exact-vs-live  max {max(dl):.3e} rms {math.sqrt(sum(v*v for v in dl)/128):.3e}")
print(f"spectrum {S}: exact-vs-Cref max {max(dc):.3e} rms {math.sqrt(sum(v*v for v in dc)/128):.3e}")
print(f"spectrum {S}: live-vs-Cref  max {max(abs(a-b) for a, b in zip(lv, rf)):.3e}")
sig = max(abs(v) for v in rf)
print(f"signal max {sig:.3e}")
for S in [3, 5, 6, 7]:
    seg = ring[448 + 128 * S:448 + 128 * S + 256]
    ex = dctiv_naive(fold(seg, ere, eim))
    lv = m3[0][128 * S:128 * S + 128]
    rf = ref[128 * S:128 * S + 128]
    ml = max(abs(a - b) for a, b in zip(ex, lv))
    mc = max(abs(a - b) for a, b in zip(ex, rf))
    mo = max(abs(a - b) for a, b in zip(lv, rf))
    print(f"spectrum {S}: exact-live {ml:.3e}  exact-Cref {mc:.3e}  live-Cref {mo:.3e}")
