#!/usr/bin/env python3
"""Generate the synthetic FLAC fixtures used by aac-convert tests.

The PCM content is generated here and contains no third-party audio. ffmpeg is
used only as an offline FLAC encoder to produce committed test inputs; it is
not a build or runtime dependency of aac-convert.
"""

from __future__ import annotations

import math
import pathlib
import struct
import subprocess
import wave

HERE = pathlib.Path(__file__).resolve().parent


def write_wav(name: str, rate: int, channels: int, bits: int, frames) -> pathlib.Path:
    path = HERE / name
    with wave.open(str(path), "wb") as wav:
        wav.setnchannels(channels)
        wav.setsampwidth(bits // 8)
        wav.setframerate(rate)
        if bits == 16:
            payload = b"".join(
                struct.pack("<h", max(-32768, min(32767, int(sample))))
                for frame in frames
                for sample in frame
            )
        elif bits == 24:
            payload = b"".join(
                struct.pack("<i", max(-8388608, min(8388607, int(sample))))[:3]
                for frame in frames
                for sample in frame
            )
        else:
            raise ValueError(bits)
        wav.writeframes(payload)
    return path


def wav_to_flac(wav: pathlib.Path, flac: pathlib.Path) -> None:
    subprocess.run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-i",
            str(wav),
            "-c:a",
            "flac",
            "-compression_level",
            "8",
            str(flac),
        ],
        check=True,
    )
    wav.unlink()


def sine_frames(rate: int, seconds: float, freq: float, peak: int):
    """Stereo 440/880 Hz-style sine generator: deterministic integer PCM."""
    total = int(rate * seconds)
    for i in range(total):
        sample = int(peak * math.sin(2.0 * math.pi * freq * i / rate))
        yield (sample, sample)


def main() -> None:
    write_wav(
        "stereo-48k-16.wav",
        48000,
        2,
        16,
        (
            (i * 2 - 12000, 12000 - i * 2)
            for i in range(12000)
        ),
    )
    wav_to_flac(HERE / "stereo-48k-16.wav", HERE / "stereo-48k-16.flac")

    write_wav(
        "mono-441k-16.wav",
        44100,
        1,
        16,
        ((((i * 7) % 20000) - 10000,) for i in range(3000)),
    )
    wav_to_flac(HERE / "mono-441k-16.wav", HERE / "mono-441k-16.flac")

    write_wav(
        "stereo-48k-24.wav",
        48000,
        2,
        24,
        (
            (i * 1000 - 500000, 500000 - i * 1000)
            for i in range(1000)
        ),
    )
    wav_to_flac(HERE / "stereo-48k-24.wav", HERE / "stereo-48k-24.flac")

    write_wav(
        "mono-32k-16.wav",
        32000,
        1,
        16,
        ((int(10000 * math.sin(i / 20.0)),) for i in range(1000)),
    )
    wav_to_flac(HERE / "mono-32k-16.wav", HERE / "mono-32k-16.flac")

    write_wav(
        "three-channel-48k-16.wav",
        48000,
        3,
        16,
        ((i * 3 - 1500, 1500 - i * 3, i) for i in range(1000)),
    )
    wav_to_flac(
        HERE / "three-channel-48k-16.wav",
        HERE / "three-channel-48k-16.flac",
    )

    source = (HERE / "stereo-48k-16.flac").read_bytes()

    # The StreamInfo MD5 starts at byte 18 of the 34-byte StreamInfo block,
    # which starts at byte 8 of the FLAC file (4-byte marker + 4-byte header).
    streaminfo_md5_offset = 26
    assert source[:4] == b"fLaC"

    truncated = source[:-100]
    (HERE / "truncated-stereo-48k-16.flac").write_bytes(truncated)

    md5_mismatch = bytearray(source)
    md5_mismatch[streaminfo_md5_offset] ^= 0xFF
    (HERE / "md5-mismatch-stereo-48k-16.flac").write_bytes(md5_mismatch)

    no_md5 = bytearray(source)
    no_md5[streaminfo_md5_offset : streaminfo_md5_offset + 16] = b"\0" * 16
    (HERE / "no-md5-stereo-48k-16.flac").write_bytes(no_md5)

    # Loudness fixtures (L3): 5-second synthetic tones so R128 gating blocks
    # clear (the 0.25 s fixtures above are unmeasurable by construction).
    write_wav(
        "loud-tone-48k-16.wav",
        48000,
        2,
        16,
        sine_frames(48000, 5.0, 440.0, 4096),
    )
    wav_to_flac(HERE / "loud-tone-48k-16.wav", HERE / "loud-tone-48k-16.flac")

    write_wav(
        "loud-quiet-48k-16.wav",
        48000,
        2,
        16,
        sine_frames(48000, 5.0, 880.0, 1024),
    )
    wav_to_flac(HERE / "loud-quiet-48k-16.wav", HERE / "loud-quiet-48k-16.flac")

    def clip_frames():
        # Quiet 440 Hz sine (peak 1638) with one near-full-scale spike pair at
        # 2.5 s: positive gain with genuine clipping risk (adjusted, not zero).
        total = 48000 * 5
        for i in range(total):
            sample = int(1638 * math.sin(2.0 * math.pi * 440.0 * i / 48000))
            if i == 120000:
                sample = 29491
            yield (sample, sample)

    write_wav("loud-clip-48k-16.wav", 48000, 2, 16, clip_frames())
    wav_to_flac(HERE / "loud-clip-48k-16.wav", HERE / "loud-clip-48k-16.flac")

    write_wav(
        "loud-silence-48k-16.wav",
        48000,
        2,
        16,
        ((0, 0) for _ in range(48000 * 2)),
    )
    wav_to_flac(HERE / "loud-silence-48k-16.wav", HERE / "loud-silence-48k-16.flac")

    write_wav(
        "loud-mono-441k-16.wav",
        44100,
        1,
        16,
        ((int(4096 * math.sin(2.0 * math.pi * 440.0 * i / 44100)),) for i in range(44100 * 5)),
    )
    wav_to_flac(HERE / "loud-mono-441k-16.wav", HERE / "loud-mono-441k-16.flac")


if __name__ == "__main__":
    main()
