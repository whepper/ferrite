#!/usr/bin/env python3
"""Deterministic FPE edge-case PCM fixtures (FPE M1, task FPE-1.4).

Additive to make_fixtures.py (which is unchanged). Covers the numerical
edge-case corpus gaps named in docs/fpe/07-validation-strategy.md §6:

  * near-silence ramp        (near-silence / low-level, quiet scaled bands)
  * full-scale square        (high-level, clip cap, polarity-inverted pair)
  * power-of-two amplitudes  (power-of-two-boundary energies, clz knife edges)
  * max-headroom bands       (full-scale impulse train + HF tone)
  * all-zero element         (zeroSpec shortcut, silent-element path)
  * mixed silent+active pair (joint shift decisions with a silent partner)

All files: 16-bit PCM WAV, 48 kHz. No randomness. Regenerating this script
must produce byte-identical output (fixed amplitudes/phases, no dither);
M1 validates this with a 3-run byte-stability check. The Rust test-time
resynthesis lives in rust/src/aac_enc/testdata.rs (fixture_samples, same
evaluation order, truncation toward zero). Oracle ring/psy vector anchoring
for these fixtures requires the C reference (blocked on re-provisioning,
risk R-12); the in-binary Q31-vs-FPE harness consumes the resynthesized PCM
directly and needs no C artifacts.
"""
import wave
import struct
import math
import os
import sys

SR = 48000

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.dirname(os.path.abspath(__file__))


def write_wav(name, channels, seconds, fn):
    n = int(SR * seconds)
    path = os.path.join(OUT, name)
    with wave.open(path, "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(SR)
        frames = bytearray()
        for i in range(n):
            t = i / SR
            for c in range(channels):
                v = max(-32768, min(32767, int(fn(t, c, i))))
                frames += struct.pack("<h", v)
        w.writeframes(bytes(frames))
    print(f"wrote {path} ({n} samples/ch)")


def sine(t, f=1000.0, amp=10000.0):
    return amp * math.sin(2.0 * math.pi * f * t)


# Near-silence ramp: 1 kHz sine whose amplitude ramps linearly 0 -> 64 LSB
# over 1 s. The first sample is exactly zero; early frames are PCM-floor
# dominated (quiet scaled bands), later frames stay far below full scale.
RAMP_N = SR  # 1.0 s
write_wav(
    "fpe-near-silence-ramp-stereo-48k.wav", 2, 1.0,
    lambda t, c, i: (64.0 * i / (RAMP_N - 1)) * math.sin(2.0 * math.pi * 1000.0 * t),
)

# Full-scale square: 500 Hz square wave hitting both rails (+32767/-32768).
# Channel 1 is the polarity inverse of channel 0: mid energy is zero while
# side energy is full-scale (MS sentinel / saturation edge).
write_wav(
    "fpe-fullscale-square-stereo-48k.wav", 2, 0.5,
    lambda t, c, i: (32767.0 if math.sin(2.0 * math.pi * 500.0 * t) >= 0.0 else -32768.0)
    * (1.0 if c == 0 else -1.0),
)

# Power-of-two boundary energies: 15 segments of 3200 samples, 1 kHz sine
# at amplitudes 2^0 .. 2^14. Band energies land on/near power-of-two
# boundaries, pressuring the integer clz headroom analysis (maxScale) and
# the rescale shift decisions at their knife edges.
POW2_SEG = 3200
write_wav(
    "fpe-pow2-boundary-stereo-48k.wav", 2, 1.0,
    lambda t, c, i: sine(t, amp=float(2 ** (i // POW2_SEG))),
)

# Max-headroom bands: full-scale impulse train (every 512 samples) plus a
# full-scale 16 kHz tone. Channel 1 offsets the impulses by 256 samples so
# the pair's joint minSpecShift/finalShift/tnsSpecShift decisions see
# different transient alignments at maximum band headroom.
def max_headroom(t, c, i):
    click = 30000.0 if (i - (256 if c == 1 else 0)) % 512 == 0 else 0.0
    return click + 30000.0 * math.sin(2.0 * math.pi * 16000.0 * t)


write_wav("fpe-max-headroom-stereo-48k.wav", 2, 0.5, max_headroom)

# All-zero element: both channels exactly zero (zeroSpec shortcut, the
# silent-element path with its -1.0 Ld placeholders and zero energies).
write_wav("fpe-allzero-stereo-48k.wav", 2, 0.25, lambda t, c, i: 0.0)

# Mixed silent+active pair: channel 0 is silent for 0.5 s then a 1 kHz sine
# at amp 10000 (onset); channel 1 stays exactly silent throughout. Exercises
# joint representation-lane decisions and MS energies with a silent partner,
# plus long/short transitions against a zero channel.
MIX_HALF = 24000
write_wav(
    "fpe-mixed-pair-stereo-48k.wav", 2, 1.0,
    lambda t, c, i: 0.0 if c == 1 or i < MIX_HALF else sine(t),
)

# Short-burst-quiet (R2.0): quiet 1 kHz sine (peak ~64 LSB) with a louder
# periodic click (amp 2000 every 2048 samples) to force short windows at low
# signal energy. Exercises the quiet short-window sentinel/PE path (Class A/B
# low-energy bands under transient block switching). Deterministic: fixed
# amplitudes, fixed period, no randomness.
write_wav(
    "fpe-short-burst-quiet-stereo-48k.wav", 2, 1.0,
    lambda t, c, i: (
        64.0 * math.sin(2.0 * math.pi * 1000.0 * t)
        + (2000.0 if i % 2048 == 0 else 0.0)
    ),
)

# TNS-active-short (R2.0): gated 8 kHz tonal bursts (0.1 s tone / 0.1 s
# silence, amp 15000) to trigger short windows at onsets while presenting
# predictable tonal content to TNS detect. Exercises short-window TNS-active
# paths on real data (production reachability: tnsSpecShift stays 0 for
# short). Deterministic: fixed frequency/amplitude/gating, no randomness.
write_wav(
    "fpe-tns-active-short-stereo-48k.wav", 2, 1.0,
    lambda t, c, i: (
        15000.0 * math.sin(2.0 * math.pi * 8000.0 * t)
        if (i % 9600) < 4800 else 0.0
    ),
)