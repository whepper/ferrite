#!/usr/bin/env python3
"""V4.35 corrected pipeline: C scales taken as tracked; exact math in live convention."""
import re, struct, math, cmath
from collections import Counter

W33 = "/private/var/folders/*/T/opencode/v4.33-ringmdct-20261020/v433-live.log"
for l in open(W33):
    m = re.match(r"V433RING entry=0 .* RING=\[(.*)\]", l.strip())
    if m:
        ring = [struct.unpack("<f", struct.pack("<I", int(x)))[0] for x in m.group(1).split(",")]
W34 = "/private/var/folders/*/T/opencode/v4.34-winstate-20261020/v434-live.log"
for l in open(W34):
    m = re.match(r"V434SLOPE entry=0 .* BITS=\[(.*)\]", l.strip())
    if m:
        lw = [struct.unpack("<f", struct.pack("<I", int(x)))[0] for x in m.group(1).split(",")]
        break
tw = [(lw[2*k], lw[2*k+1]) for k in range(64)]
th = [math.atan2(lw[2*k+1], lw[2*k]) for k in range(64)]
t0, dt = th[0], (th[-1] - th[0]) / 63
ere = [math.cos(t0 + k * dt) for k in range(64)]
eim = [math.sin(t0 + k * dt) for k in range(64)]

pre, fft = [], []
for l in open("/tmp/v435-charness/c-dct.txt"):
    t = l.split()
    if t[0] == "DCTPRE":
        pre.append((int(t[2].split("=")[1]), list(map(int, t[3:]))))
    elif t[0] == "DCTFFT":
        fft.append((int(t[2].split("=")[1]), list(map(int, t[3:]))))

def fold(seg, wre, wim):
    out = [0.0] * 128
    for k in range(64):
        out[64 + k] = (seg[k] * wim[k] - seg[127 - k] * wre[k]) * 2.0
    for i in range(64):
        j = 63 - i
        out[i] = -((seg[192 + i] * wim[j] + seg[191 - i] * wre[j])) * 2.0
    return out

def pretw(x):
    # C pre-twiddle math (/4 total), tracked exp unchanged
    d = x[:]
    o0, o1 = 0, 126
    for i in list(range(0, 62, 2)) + [62]:
        a1, a2 = d[o1 + 1], d[o0 + 0]
        a3, a4 = d[o0 + 1], d[o1 + 0]
        tr, ti = tw[i]
        n1r = (a1 * tr - a2 * ti) / 4.0
        n1i = (a1 * ti + a2 * tr) / 4.0
        tr2, ti2 = tw[i + 1]
        n3r = (a4 * tr2 - a3 * ti2) / 4.0
        n3i = (a4 * ti2 + a3 * tr2) / 4.0
        d[o0 + 0], d[o0 + 1] = n1i, n1r
        d[o1 + 0], d[o1 + 1] = n3i, -n3r
        o0 += 2
        o1 -= 2
    return d

def ratio_hist(cvals, cexp, xvals, tag):
    hist = Counter()
    worst = (0.0, -1)
    for i, (c, x) in enumerate(zip(cvals, xvals)):
        cr = c * 2.0**(cexp - 31)
        if abs(x) > 1e-6 and abs(cr) > 1e-9:
            key = round(math.log2(abs(cr / x)) * 4) / 4
            hist[key] += 1
        e = abs(cr - x)
        if e > worst[0]:
            worst = (e, i)
    print(f"{tag}: ratio-hist {dict(sorted(hist.items()))} maxabs {worst[0]:.3e} at idx{worst[1]}")

for s in range(3, 8):
    seg = ring[448 + 128 * s:448 + 128 * s + 256]
    ep = pretw(fold(seg, ere, eim))
    ee, cp = pre[s]
    print(f"--- spectrum {s} ---")
    ratio_hist(cp, ee, ep, "Cpre-vs-exactpre")
    # exact FFT of exact-pre
    xc = [complex(ep[2*m], ep[2*m+1]) for m in range(64)]
    XF = [sum(xc[m] * cmath.exp(-2j * math.pi * m * k / 64) for m in range(64)) for k in range(64)]
    ef, cf = fft[s]
    flat_c = [v * 2.0**(ef - 31) for v in cf]
    flat_x = [v for z in XF for v in (z.real, z.imag)]
    ratio_hist(flat_c, 31, flat_x, "Cfft-vs-exactfft")
