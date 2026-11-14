# F05D-1 human listening-review worksheet (manual review only)

**Status: review not performed. Every fixture below is `pending`.**
This worksheet authorizes nothing and approves nothing. Final inclusion
remains a separate decision.

Listening here is a manual content-suitability review: does the recording
contain the intended material without defects or surprises, and is a whole
file or a segment suitable for the challenge category. It is **not** an ABX
test, not a proof of perceptual equivalence, and not a quality ranking.
Existing spectral measurements (PROVENANCE.md Gate B evidence) may be
referenced but are **not** substitutes for listening, and no listening
claims have been made to date.

Playback on this machine: `afplay <file>` (macOS native CLI player;
`ffplay` also present). Do not copy audio into the repository. Do not
modify, transcode, or re-hash any file — audition only.

## Files under review (from `proposed-fixtures.json`)

| Fixture ID | Fixture file (audition this) | SHA-256 | Properties |
|---|---|---|---|
| `pf-castanets-mono-48k` | `/private/var/folders/*/T/opencode/f05b-acquisition/ch-castanets-mono-48k.wav` (= source, no derivation) | `208f9edb…ee7272` | 48 kHz / 16-bit / mono, 35.99 s |
| `pf-snare-mono-48k` | `/private/var/folders/*/T/opencode/f05b-acquisition/ch-snare-mono-48k-16bit.wav` (derived; source 24-bit preserved separately) | `729803eb…50353ab8` | 48 kHz / 16-bit / mono, 0.795 s |
| `pf-speech-vctk-p225_021` | `/private/var/folders/*/T/opencode/f05b-acquisition/ch-speech-vctk-p225_021.wav` (decoded from FLAC) | `1202119f…f7544770` | 48 kHz / 16-bit / mono, 9.03 s |
| `pf-speech-vctk-p225_003` | `/private/var/folders/*/T/opencode/f05b-acquisition/ch-speech-vctk-p225_003.wav` (decoded from FLAC) | `504b2734…0c70a9c7e` | 48 kHz / 16-bit / mono, 7.59 s |

Full hashes in `proposed-fixtures.json`. All files verified present,
readable, and hash-matching on 2026-10-26.

## Review criteria (apply to every fixture)

1. **Category match:** does the recording contain the intended type of
   material (transient attacks / exposed sibilant speech)?
2. **Obvious defects:** clipping, decoding problems, unexpected
   distortion, dropouts, or other apparent defects.
3. **Background content:** unrelated speech, music, noise, hum, or other
   unexpected content.
4. **Challenge suitability:** does the excerpt exercise the intended
   encoder behavior (pre-echo / block-switching; HF sibilant coding)?
5. **Description vs. recording:** any discrepancy between the documented
   source description and what is actually heard.
6. **Segment selection:** is the whole file suitable, or should a segment
   be selected? Record boundaries (start/end in seconds *and* samples)
   with rationale. Whole-file use is acceptable where justified.

## Fixture reviews

### `pf-castanets-mono-48k` (transient; 36 s castanet strikes)

- Reviewer:
- Review date:
- Decision: `pending`
- Observations (criteria 1–5):
- Selected segment, if any (start/end s + samples):
- Segment rationale:
- Gate B note for context (not a substitute): 814 samples >50% FS across
  all 36 s; wideband to 21.6 kHz, no brick wall; one full-scale sample.

### `pf-snare-mono-48k` (transient; 0.79 s snare hit)

- Reviewer:
- Review date:
- Decision: `pending`
- Observations (criteria 1–5):
- Selected segment, if any (start/end s + samples):
- Segment rationale:
- **Snare conversion note:** the fixture is derived from the 24-bit
  original by arithmetic right-shift truncation (`>> 8`), **without
  dithering**. Do not presume this conversion is transparent. Note any
  audible issues (grain, noise modulation, altered attack) without
  assuming they exist. The 24-bit original is preserved for comparison.
- Gate B note for context (not a substitute): single hit at t=0, sharp
  onset, LF-heavy body with decaying HF rattle; 0.795 s actual
  (page rounds to 00:01).

### `pf-speech-vctk-p225_021` (vocal; rainbow passage, ~9 s)

- Reviewer:
- Review date:
- Decision: `pending`
- Observations (criteria 1–5):
- Selected segment, if any (start/end s + samples):
- Segment rationale:
- Gate B note for context (not a substitute): transcript on file
  ("The difference in the rainbow depends considerably ...");
  sibilant bursts to ~47% of fullband RMS in 4–12 kHz; no brick wall.

### `pf-speech-vctk-p225_003` (vocal; elicitation passage, ~7.6 s)

- Reviewer:
- Review date:
- Decision: `pending`
- Observations (criteria 1–5):
- Selected segment, if any (start/end s + samples):
- Segment rationale:
- Gate B note for context (not a substitute): transcript on file
  ("Six spoons of fresh snow peas, ..."); densest sibilants of the
  screened set; 7.588 s duration.

## Rules for this review

- Leave `pending` unless the evidence heard clearly supports `pass` or
  `reject`; do not pre-fill observations.
- A `pass` here means content-suitable for the category, not technically
  accepted (Gate B already covered), not approved for inclusion.
- Record timestamps in both seconds and samples (rate 48000 Hz).
- Do not modify any audio file. Do not add fixtures. Do not commit.
