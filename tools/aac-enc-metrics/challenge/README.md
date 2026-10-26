# F05 challenge corpus (design-only)

A separate, reproducible AAC-LC VBR-5 challenge corpus to complement the
frozen 19-fixture regression corpus (`../fixtures-v3.json`). F05B acquired
and Gate B verified three candidates' source audio (originals preserved
unmodified outside the repo); **no fixture is approved, nothing here is
measured.**

Status: `design-only`. Approved fixture count: **0**.
See `manifest.json` (schema) and `PROVENANCE.md` (candidate table and rights
review). Do not add these IDs to `fixtures-v3.json`.

## Why a separate corpus

The frozen corpus is 15 synthetic fixtures plus 3 licensed (m7) plus 1
provisional real-music fixture (m8). It underrepresents real-instrument
transients, sustained tonal material, true-stereo images, exposed voice, and
dense mixes. The challenge corpus targets exactly those behaviors so future
Rust-vs-C comparisons can localize differences instead of re-proving the
baseline.

## Proposed set: 10 candidates, none approved

Lossless containers are necessary but not sufficient. The bar tightened
2026-10-26: every candidate must also show genuine full-band source quality
and documented lossless provenance. A 48 kHz FLAC may still originate from a
lossy or bandwidth-limited source, so container format, recording chain,
bandwidth, prior processing, and licence are assessed as separate
dimensions (see `manifest.json` v3 status fields and PROVENANCE.md).
Upsampling a low-rate source is forbidden as a qualification strategy.
Rejected candidates and the reasons stay recorded in `manifest.json`.
The table below is **proposed coverage, not a validated corpus**: every
entry is unresolved, and unresolved entries count as gaps, not coverage
(see Remaining gaps). Legacy booleans in the manifest (`lossless`,
`redistribution_permitted`, `conversion_permitted`) are null until verified
and never imply approval; the gate decisions below are authoritative.

| Fixture ID | Category | Source (licence) | Diagnostic value |
|---|---|---|---|
| `ch-castanets-mono-48k` | transient | BigSoundBank SARDIN (CC0; Gate A pass) | Real HF attacks; pre-echo / block-switching |
| `ch-snare-mono-48k` | transient | BigSoundBank SARDIN (CC0; Gate A pass) | Noise burst + shell resonance; different transient class |
| `ch-piano-solo-stereo` | tonal | Musopen / Xuan He Chopin (PD/PD; 44.1 kHz exploratory) | Sustained piano tones and decays; tonal noise |
| `ch-bells-stereo-48k` | tonal + stereo | Freesound klankbeeld (CC0; Gate A pass) | Inharmonic bell partials; MS stereo ambience; HF-rich |
| `ch-ambience-stereo-48k` | stereo | Freesound felix.blume (CC0) | Diffuse bed + discrete events; joint-stereo behavior |
| `ch-asym-stereo` | stereo | Freesound Ionizing (CC0) | Asymmetric image; balance / mid-side stress; 44.1 kHz so exploratory |
| `ch-speech-vctk-48k` | vocal | CSTR VCTK v0.92, Edinburgh (CC BY 4.0, primary-verified; Gate A pass, audio-only) | Full-band speech from 96 kHz chamber originals; replaces rejected LibriSpeech |
| `ch-voice-clean-48k` | vocal | Freesound AcoustiAngel (CC0) | Full-band clean 48 kHz voice (consent flag: review first) |
| `ch-soprano-solo-flac` | vocal + tonal | HMV Schumann 1934/35 (PDM, **US rights blocker; 192 kHz transfer of shellac**) | Exposed singing; blocked on rights and era bandwidth |
| `ch-orchestra-tutti-flac` | dense | Musopen Symphony Eroica (PD/PD; Gate A blocked, licence basis) | Full tutti; masking interactions; sibling of the m8 precedent, different passage |

Count justification: 10 covers all 5 required categories with no two
fixtures testing the same behavior. Bells and soprano each serve two
categories by design (dual value, not duplication). The corpus stays small
enough to review fixture-by-fixture. Quality outranks coverage: the 16 kHz
LibriSpeech speech candidate was rejected rather than upsampled, and vocal
coverage now rests on not-yet-verified candidates (see Remaining gaps).

## Coverage matrix

| Category | Fixtures | Currently underrepresented behavior |
|---|---|---|
| Sharp transients / pre-echo | ch-castanets, ch-snare | Real-instrument attacks (synthetic clicks only in frozen set) |
| Tonal / sustained | ch-piano, ch-bells, ch-soprano | Real decays, inharmonic partials, sung vowels |
| Stereo edge cases | ch-bells (MS), ch-ambience (ORTF), ch-asym | True stereo, MS/ORTF correlation, asymmetry (m7 speech is dual-mono) |
| Exposed vocals / sibilants | ch-speech-vctk, ch-voice-clean, ch-soprano | Full-band speech (Gate B pass, assembly outstanding), full-band voice (consent flag), singing (rights + era-bandwidth blockers) |
| Dense music | ch-orchestra-tutti, ch-soprano | Tutti masking; m8 is a single 10 s passage |

## Repository layout (separate from the regression corpus)

```text
tools/aac-enc-metrics/challenge/
  README.md       # this file: design, coverage, evaluation plan
  manifest.json   # schema v3: 0 approved fixtures, 10 needs-review candidates, 10 rejected; per-candidate gate states; nullable booleans
  PROVENANCE.md   # candidate table, rights review, conversion policy, acquisition checklist
```

`fixtures-v3.json` is untouched and remains the only corpus any tooling
reads. No F04 tool change is part of this task.

## Authorization gates

The workflow is split so that file-dependent checks never gate authorization
and authorization never precedes evidence:

**Gate A — pre-download authorization.** Confirm the exact source file,
verify licence, permitted acquisition, and intended use from primary
evidence, and resolve all rights, consent, content, and redistribution
blockers **before** obtaining bytes. Record remaining uncertainties; never
authorize a blocked candidate to enable investigation.

**Gate B — post-download technical acceptance.** Only after Gate A passes,
acquire the authorized file and verify bytes, checksum, format, rate,
bit depth, channels, duration, and source identity; inspect bandwidth and
content suitability including audition; verify conversion history and
category fit. Reject or return to review on contradiction.

Current state (F05A pre-download review, 2026-10-26): **Gate A pass**
for `ch-speech-vctk-48k` (audio-only), `ch-castanets-mono-48k`,
`ch-snare-mono-48k`, `ch-bells-stereo-48k`; **blocked** for
`ch-orchestra-tutti-flac` (recording-licence basis unresolvable from
primary sources); **pending** for piano, ambience, soprano, asym, and
voice-clean (not re-reviewed this phase).

Current state (F05B acquisition, 2026-10-26): **Gate B pass** for
`ch-castanets-mono-48k` and `ch-snare-mono-48k` (bytes acquired from the
authorized URLs, headers/spectra/content verified, originals preserved
unmodified) and for `ch-speech-vctk-48k` (inner ZIP MD5 exact match;
p225_021/p225_003 mic1 utterances verified: FLAC 48/16/mono headers,
clean decode, genuine wideband spectra with exposed sibilants,
B1-readable 16-bit PCM); `ch-bells-stereo-48k` acquisition **failed** on the
Freesound login wall (endpoint returns the login page; no credentials, no
workaround — Gate B not performed, Gate A stands). Gate B pass does not
approve final corpus inclusion; that awaits a corpus-level review.

## Reproducible acquisition and conversion policy

1. **Lossless gate, plus provenance.** Acquire the original lossless file
   only (Commons original FLAC, Freesound WAV/FLAC, corpus-distributed
   files). Never transcode a lossy copy into the corpus. Verify the
   container is lossless at ingest (`ffprobe` / FLAC metadata) **and**
   assess whether the content is genuinely full-band: check the header rate
   against a spectral bandwidth inspection for codec cutoffs, brick-wall
   resampling artifacts, or era-limited bandwidth. Reject on any lossy
   perceptual-coding step or unsupported bandwidth claim. Documented
   producer-side sample-rate conversion or bit-depth reduction is a
   different case (see policy clarification below), not a "lossy step".
2. **No silent transforms.** No resampling, downmix, bit-depth change,
   loudness normalization, trim, fade, or leading-silence edit without a
   documented, deterministic procedure. Any required conversion uses a pinned
   tool build (record `ffmpeg -version` hash equivalent: version string plus
   binary SHA-256, as in m8-PROVENANCE.txt), explicit flags
   (`-fflags +bitexact -flags:a +bitexact`, no dither unless stated), and the
   exact command in the provenance record.
3. **Allowed conversions and their rules.**
   - Excerpt: exact sample-range trim (`atrim=start_sample:end_sample`),
     recorded as sample offsets and seconds.
   - 24-bit to 16-bit: arithmetic shift, no dither, verified sample-by-sample
     (m8 precedent: `v24 >> 8` equality over all samples).
   - Upsampling a bandwidth-limited source to qualify it is
     **forbidden**. A 16 kHz recording stays a 16 kHz recording: it is
     rejected from this corpus, not resampled into it. (The LibriSpeech
     candidate was retired under this rule.)
   - Mono to dual-mono (if chosen): bit-exact channel duplication, verified.
   - 44.1 kHz (ch-asym, and any native-44.1 kHz source): stays
     exploratory; resampling it to 48 kHz to chase the gate is forbidden.
     The gate is extended only by separate review, never by conversion.
4. **Hash verification.** Record source checksum (as published), acquired-file
   checksum, and final WAV PCM checksum (`input_sha256` equivalent). Any
   mismatch fails acquisition; never substitute a re-encode.
5. **Gate A before bytes.** No download where rights are unclear
   (ch-soprano blocker, ch-asym content flag, ch-voice-clean consent flag)
   or where per-file checks are outstanding (all candidates, including
   VCTK). Re-resolve every source page at acquisition time; pages change.
   Gate A review is a per-candidate, recorded decision — not the mere
   existence of licence evidence in these files.

**Processing-policy clarification.** Three cases, three verdicts:
   - Lossy perceptual codecs (MP3, Vorbis, AAC) or undocumented processing
     that destroys bandwidth: **unacceptable**, always rejected.
   - Documented producer-side sample-rate conversion or bit-depth reduction
     (VCTK's stated 96-to-48 kHz distribution downsampling; a recordist's
     stated 96 kHz original served at 48 kHz): **acceptable only if** the
     history is documented, the resulting band fits the intended category,
     and Gate B spectral inspection confirms genuine content. This resolves
     the apparent conflict: the "lossy step" ban targets perceptual coding
     and undocumented bandwidth destruction, not documented linear
     conversion. No conversion is declared harmless — VCTK's downsampling
     filter is itself undocumented, so Gate B must confirm the 48 kHz files
     carry genuine full-band content before acceptance.
   - Upsampling: **never** restores lost bandwidth or detail, and is
     forbidden as a qualification strategy (see rule 3).

## Remaining gaps (2026-10-26; proposed coverage, not validated)

- **Exposed vocals / sibilants:** no approved fixture. VCTK passed Gate B
  (inner ZIP MD5 exact match; p225_021/p225_003 mic1 verified: genuine
  wideband spectra, exposed sibilant bursts, B1-readable 16-bit PCM) but
  corpus assembly (excerpt + dual-mono derivation) and final inclusion
  review are outstanding; AcoustiAngel carries
  a consent flag; soprano is blocked on rights and, independently, is not
  suitable for high-frequency sibilance testing (1934 shellac transfer,
  era-bandwidth-limited regardless of rights). If
  final review rejects VCTK, the category has no speech candidate
  and the gap stands.
- **Tonal / sustained:** ch-piano is API-confirmed 44.1 kHz/16-bit
  (exploratory; original ADC chain undocumented) and ch-soprano is blocked;
  only ch-bells has a strong documented chain. A second verified tonal
  source is desirable but not yet established.
- **Dense music:** ch-orchestra-tutti-flac is Gate A **blocked**: the
  Musopen dedication vs ToS-warning conflict could not be resolved from
  primary sources (musopen.org bot-blocked; Archive.org PDM item and
  unchallenged hosting noted but insufficient alone). File fixed to the
  API-verified movement I FLAC; resolvers documented in PROVENANCE.md.
- **Stereo edge cases:** the asymmetric candidate's background "song"
  ambiguity is unresolved from text alone (no music/voice tags; audition or
  uploader clarification required), so asymmetry coverage is proposed only.
  The bells candidate (MS stereo) is Gate A authorized but not acquired
  (Freesound login wall; Gate B not performed).
- **File-level history:** several candidates lack verified file-level
  encoding or conversion history (served-file headers, uploader-side
  conversions, re-encode tags). These verify at Gate B or the candidates
  do not enter the corpus.

## Proposed fixture set (F05C, pending final review)

`proposed-fixtures.json` (schema v1) proposes four fixtures from the three
Gate-B-passed candidates: castanets source as-is (48/16/mono, 35.99 s),
snare derived to 16-bit via verified arithmetic shift (no dither, transient
intact, original untouched), and two VCTK utterances decoded to 16-bit PCM
WAV via pinned ffmpeg 9.0.2 (binary hash identical to the m8 build).
All four are B1-readable; all are `proposed-pending-final-review` —
**not approved, not in the baseline, nothing measured.** Listening review
is outstanding (checklist in PROVENANCE.md; worksheet in `listening-review-worksheet.md`); no listening claims are made.

## Future evaluation plan (uses existing F04 tooling unchanged)

- Keep the 19-fixture baseline and its eligibility rules frozen. Challenge
  results are stored in a separate results directory and never merged into
  regression reports.
- Reuse B1 provenance and alignment safeguards verbatim
  (`vbr5_metrics.py` manifest validation, input SHA-256 checks, nominal-1600
  alignment policy, decoded-length plausibility). Challenge fixtures must
  satisfy the same per-stream gates to be `gated`; 44.1 kHz and any
  provisional provenance stay `exploratory` unless the gate is formally
  extended in a separate reviewed task. Sample-rate compatibility is a
  technical property, not a quality verdict and not an approval: an
  exploratory fixture is still fully usable for B2 localization, it is
  simply reported under its own scope, never pooled with gated results.
- Use B2 (`vbr5_localize.py`) to localize differences in time and
  (optionally) spectral bands. No pass/fail thresholds, no single quality
  score, no perceptual-equivalence / audibility / transparency claims from
  objective numbers. No ABX or listening tests in this task.
- A future report summarizes, per fixture: coverage intent, input provenance
  (source, licence, segment, conversion command, hashes), encoded size and
  framing, decoded error (SNR/RMS/peak with B1-identical formulas), and
  optional B2 localization. Challenge and regression results are presented
  side by side, never pooled.
