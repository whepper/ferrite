# M7.3 test fixtures

All FLAC files in this directory were generated from synthetic PCM written by
`generate.py`; they contain no third-party recordings or other copyrighted
audio. `ffmpeg` was used only as an offline FLAC encoder to create these
committed test inputs. It is not a build or runtime dependency of
`aac-convert`.

The files are intentionally tiny:

| File | Purpose |
|---|---|
| `stereo-48k-16.flac` | Supported stereo 48 kHz, 16-bit end-to-end input (12,000 frames) |
| `mono-441k-16.flac` | Supported mono 44.1 kHz, 16-bit input |
| `stereo-48k-24.flac` | Explicitly rejected 24-bit input |
| `mono-32k-16.flac` | Explicitly rejected 32 kHz input |
| `three-channel-48k-16.flac` | Explicitly rejected three-channel input |
| `truncated-stereo-48k-16.flac` | Last 100 bytes removed; conversion must fail without publishing |
| `md5-mismatch-stereo-48k-16.flac` | Audio unchanged, StreamInfo MD5 deliberately altered; checksum verification must fail |
| `no-md5-stereo-48k-16.flac` | Audio unchanged, StreamInfo MD5 zeroed; valid audio must be accepted with checksum verification explicitly unavailable |

Regenerate with:

```sh
python3 generate.py
```

`md5-mismatch-stereo-48k-16.flac` and `no-md5-stereo-48k-16.flac` intentionally
share the original valid audio data; only the 16-byte StreamInfo MD5 field is
changed. `truncated-stereo-48k-16.flac` instead removes the last 100 bytes and
is a transport/decode failure, not a checksum failure.

`generate.py` requires `ffmpeg` on `PATH`. Regeneration is an offline fixture
maintenance step, not part of the normal build or test workflow.
