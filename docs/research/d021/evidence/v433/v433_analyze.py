#!/usr/bin/env python3
"""V4.33 ring/kernel-input parity: live bit-words vs PCM mapping vs committed ring."""
import re, struct

TD = "/Users/USER/Documents/VSCode/fdk-aac/fdk-aac/rust/src/aac_enc/testdata"
LOG = "/private/var/folders/*/T/opencode/v4.33-ringmdct-20261020/v433-live.log"
WAVS = "/private/var/folders/*/T/opencode/phase0-20261015/wavs"

rings = []
for l in open(LOG):
    m = re.match(r"V433RING entry=(\d+) seq=(\d) shape=(\d) slope=(\d+) RING=\[(.*)\]", l.strip())
    if m:
        e, sq, sh, sl = int(m.group(1)), int(m.group(2)), int(m.group(3)), int(m.group(4))
        rings.append((e, sq, sh, sl, [struct.unpack("<f", struct.pack("<I", int(x)))[0]
                                      for x in m.group(5).split(",")]))
print(f"entries: {len(rings)}")
for e, sq, sh, sl, r in rings:
    print(f"entry{e}: seq={sq} shape={sh} slope={sl} len={len(r)}")

import wave
w = wave.open(f"{WAVS}/m2-onset-stereo-48k.wav", "rb")
nch, sw, sr, n = w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
raw = w.readframes(n)
import array
pcm = array.array("h", raw)
pcm = [pcm[i * nch:(i + 1) * nch] for i in range(n)]
ch0 = [s[0] for s in pcm]
ch1 = [s[1] for s in pcm]
print(f"wav: {nch}ch {sw*8}bit {sr}Hz {n} frames; ch0==ch1: {ch0 == ch1}")

# PCM-mapping check: ring[1600+i] should be frames[24][i]/32768; frames[f] = pcm[f*1024:(f+1)*1024]?
F = 24
for e, sq, sh, sl, r in rings:
    exp_tail = [ch0[F * 1024 + i] / 32768.0 for i in range(448)]
    got_tail = r[1600:2048]
    d = sum(1 for a, b in zip(got_tail, exp_tail) if a != b)
    print(f"entry{e} tail[1600:2048] vs pcm[{F*1024}:{F*1024+448}]/32768: diffs {d}/448")
    exp_hist = [ch0[F * 1024 - 1600 + i] / 32768.0 for i in range(1600)]
    got_hist = r[0:1600]
    d2 = [(i, a, b) for i, (a, b) in enumerate(zip(got_hist, exp_hist)) if a != b]
    print(f"entry{e} hist[0:1600] vs pcm[{F*1024-1600}:{F*1024}]/32768: diffs {len(d2)}/1600"
          + (f" first idx{d2[0][0]} live {d2[0][1]!r} exp {d2[0][2]!r}" if d2 else ""))

# Committed ring (ch0, i16) vs live ring ch0
nums = open(f"{TD}/ring_onset-stereo_f24_ch0.txt").read().split()
print("committed ring count:", len(nums))
cref = list(map(int, nums))
assert len(cref) == 2048
for e, sq, sh, sl, r in rings[:1]:
    back = [int(round(v * 32768.0)) for v in r]
    d = [(i, a, b) for i, (a, b) in enumerate(zip(back, cref)) if a != b]
    print(f"live-entry{e} round(ring*32768) vs committed i16: diffs {len(d)}/2048"
          + (f" first idx{d[0][0]} live {d[0][1]} ref {d[0][2]}" if d else ""))
    import math
    ma = max(abs(a - b) for a, b in zip(back, cref)) if d else 0
    print("maxabs i16-domain diff:", ma)

# ch0 vs ch1 live rings
if len(rings) == 2:
    d = sum(1 for a, b in zip(rings[0][4], rings[1][4]) if a != b)
    print(f"live ch0-ring vs ch1-ring: diffs {d}/2048")
