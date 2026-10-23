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


if __name__ == "__main__":
    main()
