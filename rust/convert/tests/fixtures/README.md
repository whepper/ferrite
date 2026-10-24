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
| `stereo-48k-24.flac` | Supported stereo 48 kHz, 24-bit input (1,000 frames); converts with a per-file precision warning |
| `mono-441k-24.flac` | Supported mono 44.1 kHz, 24-bit input (1,000 frames, nonzero low 8 bits throughout) |
| `boundary-48k-24.flac` | Stereo 48 kHz, 24-bit hand-picked edge vector (72 frames): full-scale ±, -1/+1, silence, low-bit values, DC pair, and a channel-asymmetric ramp for exact truncation checks |
| `mono-32k-16.flac` | Explicitly rejected 32 kHz input |
| `three-channel-48k-16.flac` | Explicitly rejected three-channel input |
| `truncated-stereo-48k-16.flac` | Last 100 bytes removed; conversion must fail without publishing |
| `md5-mismatch-stereo-48k-16.flac` | Audio unchanged, StreamInfo MD5 deliberately altered; checksum verification must fail |
| `no-md5-stereo-48k-16.flac` | Audio unchanged, StreamInfo MD5 zeroed; valid audio must be accepted with checksum verification explicitly unavailable |
| `tagged-core-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus all ten core Vorbis comments (`date` and `year` both present; `date` must win) |
| `tagged-core-441k-16.flac` | Same audio as `mono-441k-16.flac` plus `title`/`artist`/`album`/`tracknumber` (`3/9`)/`date` |
| `tagged-edge-48k-16.flac` | Case/whitespace/multi-value handling, `year`-only dating, `tracktotal`/`disctotal` fallbacks, and deferred keys (`bpm`, `compilation`, `grouping`, MusicBrainz ID, `label`) that must stay unmapped |
| `tagged-invalid-48k-16.flac` | Unparseable `tracknumber`/`discnumber` and whitespace-only `comment`; those atoms must be skipped with warnings while valid tags are still written |
| `tagged-unicode-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus padded non-ASCII `title` (`café 🎵`) and `comment`; stored values must trim to the intended Unicode strings |
| `tagged-cover-jpeg-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus front-cover JPEG and `title`/`artist`; must embed `covr` (type 13) last |
| `tagged-cover-png-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus front-cover PNG and `title`; must embed `covr` (type 14) last |
| `tagged-cover-unsupported-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus front-cover GIF and `title`; GIF must be skipped with a warning and no `covr` written |
| `tagged-cover-legacy-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus only a legacy `COVERART` comment and `title`; must fall back to `covr` without leaking `coverart` into unmapped keys |
| `tagged-cover-multi-48k-16.flac` | Same audio as `stereo-48k-16.flac` plus back-cover JPEG then front-cover PNG and `title`; the front PNG must win |
| `cover-jpeg-48k-24.flac` | Same audio as `stereo-48k-24.flac` plus front-cover JPEG and `title`; 24-bit precision warning must lead cover behavior |
| `cover-unsupported-48k-24.flac` | Same audio as `stereo-48k-24.flac` plus front-cover GIF and `title`; asserts the full warning order (precision → text tags → cover) with no `covr` |
| `loud-tone-48k-16.flac` | Stereo 48 kHz 5 s 440 Hz sine, peak 4096: measurable loudness, positive gain without clipping risk (L3) |
| `loud-quiet-48k-16.flac` | Stereo 48 kHz 5 s 880 Hz sine, peak 1024: quieter second album member (L3 album aggregation) |
| `loud-clip-48k-16.flac` | Stereo 48 kHz 5 s quiet 440 Hz sine (peak 1638) with one 29491 spike at 2.5 s: positive gain with genuine clipping risk, adjusted down (L3 clipping protection) |
| `loud-silence-48k-16.flac` | Stereo 48 kHz 2 s digital silence: unmeasurable, loudness atoms omitted with warnings (L3 incomplete-album rule) |
| `loud-mono-441k-16.flac` | Mono 44.1 kHz 5 s 440 Hz sine, peak 4096: rate/channel lattice for analysis (L3) |
| `loud-tagged-48k-16.flac` | Same audio as `loud-tone-48k-16.flac` plus `title`/`artist` Vorbis comments and the front-cover JPEG copied from `tagged-cover-jpeg-48k-16.flac`: text + `covr` + loudness `ilst` coexistence and ordering (L3) |

Regenerate with:

```sh
python3 generate.py
```

Then re-apply tags and cover art to `loud-tagged-48k-16.flac` (offline
`mutagen` on the committed inputs only; never a build or runtime
dependency):

```sh
cp loud-tone-48k-16.flac loud-tagged-48k-16.flac
python3 -c "
from mutagen.flac import FLAC
src = FLAC('tagged-cover-jpeg-48k-16.flac')
dst = FLAC('loud-tagged-48k-16.flac')
dst['title'] = ['Loud Title']
dst['artist'] = ['Loud Artist']
dst.add_picture(src.pictures[0])
dst.save()
"
```

`md5-mismatch-stereo-48k-16.flac` and `no-md5-stereo-48k-16.flac` intentionally
share the original valid audio data; only the 16-byte StreamInfo MD5 field is
changed. `truncated-stereo-48k-16.flac` instead removes the last 100 bytes and
is a transport/decode failure, not a checksum failure.

The `tagged-*.flac` fixtures share the audio of `stereo-48k-16.flac` (or
`mono-441k-16.flac`); only Vorbis comments were added with mutagen, so their
StreamInfo audio MD5 still verifies. They are maintained by hand, not by
`generate.py`.

The 24-bit fixtures (`stereo-48k-24.flac`, `mono-441k-24.flac`,
`boundary-48k-24.flac`) were generated with `generate.py`'s `write_wav` /
`wav_to_flac` helpers (same synthetic-PCM process, no third-party audio);
only committed files were kept, so running `generate.py` itself is not
required and would rewrite every fixture.

`generate.py` requires `ffmpeg` on `PATH`. Regeneration is an offline fixture
maintenance step, not part of the normal build or test workflow.
