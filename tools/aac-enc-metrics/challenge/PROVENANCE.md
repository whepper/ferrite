# F05 challenge-corpus provenance (design-only)

Audio acquired for three candidates under Gate A (see Gate B evidence);
no fixture approved. Every candidate below is
`needs-review`; the `rejected` list records what was excluded and why.
Source pages were inspected 2026-10-26 via fetched page text; licence and
technical claims report only what was seen. Gaps are marked needs-review,
never filled by assumption. Re-verify every page at acquisition time.
Coverage in this document is **proposed, not validated**: unresolved
candidates are gaps, not coverage. F05A pre-download review (2026-10-26):
Gate A pass for VCTK (audio-only), castanets, snare, bells; blocked for
tutti (licence basis); piano/ambience/soprano/asym/voice-clean not
re-reviewed. F05B acquisition (2026-10-26): Gate B pass for castanets,
snare, and VCTK utterances p225_021/p225_003 (mic1); bells acquisition
failed on the login wall (Gate B not performed). Acquired bytes are
preserved unmodified outside the repo; no fixture approved.

## Candidate table

| Fixture ID | Category | Recording / source page | Licence (evidence) | Redistribution | Technical suitability | Decision |
|---|---|---|---|---|---|---|
| `ch-castanets-mono-48k` | transient | Castanets (simple), J. SARDIN, BigSoundBank #922. https://bigsoundbank.com/castanets-simple-s0922.html | CC0-1.0. Page: "CC0 (public domain): Free and royalty-free". Licences page confirms CC0-1.0 Universal. | Permitted, incl. derivatives/commercial. Attribution not required. | Mono 48 kHz WAV offered, ~36 s. Verify PCM at ingest. | Gate A: pass |
| `ch-snare-mono-48k` | transient | Snare drum #6, J. SARDIN, BigSoundBank #2309. https://bigsoundbank.com/snare-drum-6-s2309.html | CC0-1.0, same wording as above. | Permitted as above. | Mono 48 kHz 24-bit, 1 s. Deterministic 24->16 shift required. | Gate A: pass |
| `ch-piano-solo-stereo` | tonal | Chopin Polonaise Op.26 No.1, Xuan He, Musopen. https://commons.wikimedia.org/wiki/File:Chopin_-_Polonaise_No._1_in_C-sharp_minor,_Op._26_No._1_(Xuan_He).flac | PD recording (Musopen dedication: "for any purpose, without any conditions") + PD-old composition (d.1849) + pre-1931 US publication. | Permitted. Attribution not required. | FLAC 8:28; Commons API metadata 44.1 kHz stereo 16-bit (exploratory). 2019 re-encode tag; ADC chain undocumented. Use original FLAC only. | Candidate |
| `ch-bells-stereo-48k` | tonal + stereo | Church bells 220302_033, klankbeeld. https://freesound.org/people/klankbeeld/sounds/622816/ | CC0-1.0. Badge plus "copy, modify, distribute and perform ... even for commercial purposes". | Permitted, incl. conversion/excerpting. | FLAC 48 kHz 24-bit stereo, 14:13, MS chain. 24->16 shift required. Page shows both "2022" and "March 2nd 2021" (verbatim date inconsistency; content identification unaffected). | Gate A: pass |
| `ch-ambience-stereo-48k` | stereo | Frogs/Crickets/Birds Rorota Trail, felix.blume. https://freesound.org/people/felix.blume/sounds/868981/ | CC0-1.0, same Freesound wording. | Permitted. | WAV 48 kHz 24-bit stereo, 11:39; description states a 96 kHz original, so the served 48 kHz implies an undocumented uploader-side conversion. Confirm served-file rate at ingest. | Candidate |
| `ch-asym-stereo` | stereo | church_bell_song.wav, Ionizing. https://freesound.org/people/Ionizing/sounds/19167/ | CC0-1.0 badge (2006 upload). | Permitted per CC0. | WAV 44.1 kHz 16-bit stereo: exploratory under current gate. Content flag: "song" ambiguous, audition first. | Candidate, elevated review |
| `ch-speech-vctk-48k` | vocal | CSTR VCTK Corpus v0.92, Univ. of Edinburgh CSTR. https://datashare.ed.ac.uk/handle/10283/3443 | CC BY 4.0 primary-verified (publisher `dc.rights` metadata + shipped `README.txt` + shipped `license_text.txt` full legal code). Attribution required. | Permitted with attribution; per-file checks outstanding. | 96 kHz/24-bit chamber originals (DPA 4035 + MKH 800), distributed 48 kHz/16-bit by documented producer downsampling. Avoid p280/p315. | Gate A: pass (audio-only). Gate B: pass (p225_021/p225_003 mic1 verified) |
| `ch-voice-clean-48k` | vocal | AcoustiAngel clean talking voice. https://freesound.org/people/AcoustiAngel/sounds/739037/ | CC0-1.0, same Freesound wording; description confirms CC0. | Permitted. | WAV 48 kHz stereo 2:07. Confirm channels/PCM at ingest. Consent flag: review first. | Candidate, elevated review |
| `ch-soprano-solo-flac` | vocal + tonal | Schubert Ave Maria D.839, E. Schumann 1934/35, HMV DB 2291. https://commons.wikimedia.org/wiki/File:Hmv-db2291-32-4807-2wx766.flac | PDM 1.0 asserted; pool page claims worldwide expiry under 50-year rule. **US status under the Music Modernization Act (100-year term) unaddressed. Blocking question.** | NOT confirmed: stays `needs-review`. | FLAC 4:33; Commons API metadata 192 kHz stereo 24-bit transfer of 1934 shellac (mono-native, ~5-8 kHz). Rights blocked; era-bandwidth-limited regardless. | Candidate, blocked |
| `ch-orchestra-tutti-flac` | dense | Beethoven Eroica, Czech NSO / Musopen Symphony 2012. https://commons.wikimedia.org/wiki/File:Beethoven_-_Symphony_No._3_in_E_flat_major,_Op._55_%27Eroica%27_-_I._Allegro_con_brio_(Musopen_Symphony).flac | PD recording (Musopen dedication) + PD-old composition (d.1827) + pre-1931 US publication. Cleanest split in set. | Permitted, no conditions. | FLAC 15:12; Commons API metadata 48 kHz stereo 24-bit. Gated path pending per-file probe. Different movement/passage than m8. | Gate A: blocked |

## Rights review

- **Approved for acquisition: none.** Approval requires, per fixture:
  re-fetched source page, resolved direct asset URL, downloaded bytes with
  published-checksum match, header probe, audition, recorded segment, final
  WAV hash, and reviewer sign-off. None of that has happened.
- **Needs-review (all 10):** evidence inspected but file-level
  verification outstanding for every candidate. F05A Gate A outcomes:
  - `ch-speech-vctk-48k`: **Gate A pass (audio-only); Gate B pass.**
    README.txt/license_text.txt re-fetched: the CC BY 4.0 grant carries no
    per-component scope language and the Herald permission note is
    unscoped, so the pass covers audio recordings only (no transcripts
    redistributed; utterance IDs only). Exact distributable identified
    (10.94 GB ZIP, MD5-pinned); inner ZIP MD5 verified exact on download.
    p225_021 (rainbow, 9.03 s) + p225_003 (elicitation, 7.59 s), mic1:
    FLAC STREAMINFO 48/16/mono, clean decode, genuine wideband spectra
    with exposed sibilant bursts, B1-readable 16-bit PCM. Mic identity and
    converter/filter details remain undocumented; corpus assembly
    (excerpt + dual-mono) and final inclusion review outstanding.
  - `ch-castanets-mono-48k`, `ch-snare-mono-48k`: **Gate A pass.** Pages
    re-fetched: WAV options, specs, and CC0 wording identical in
    substance; licences page unchanged; no new restrictions or content
    flags. Served-file headers deferred to Gate B by design.
  - `ch-bells-stereo-48k`: **Gate A pass.** Page re-fetched: CC0 badge and
    grant, FLAC 48/24 stereo specs, MS gear chain all confirmed; download
    login-gated; 2022/2021 date inconsistency persists (non-blocking).
    Gate B confirms served bytes and genuine 48 kHz content.
  - `ch-orchestra-tutti-flac`: **Gate A blocked.** musopen.org
    (work/FAQ/ToS) bot-blocked and Wayback unreachable, so the
    dedication-vs-ToS conflict cannot be closed from primary sources.
    Supporting (not sufficient): Archive.org PDM 1.0 item explicitly
    listing Eroica, byte-size/duration-identical source file, ~2 years
    unchallenged Commons hosting with named performer, m8 movement-III
    precedent. Resolvers: readable Musopen per-work badge, archived
    ToS/FAQ#4 text, or rights-holder statement covering Eroica/CNSO.
  - Piano/ambience/soprano/asym/voice-clean: not re-reviewed this phase;
    prior blockers and flags stand unchanged.
- **Composition vs recording:** splits are explicit for the classical
  fixtures (PD-old compositions + PD/CC0 recordings). Field recordings and
  SFX have no composition layer. The soprano split is the one unclear case.
- **Share-alike excluded:** the Commons drum-kit Ogg (BY-SA 3.0 + GFDL)
  was rejected partly on copyleft friction; no share-alike source is in the
  set.
- **Rejected (10):** the 2026-10-26 revision retired the LibriSpeech
  16 kHz speech candidate (bandwidth-limited; upsampling forbidden as a
  qualification strategy), replacing it with the VCTK candidate above.
  Earlier rejections: 5 lossy sources excluded by the lossless gate (MP3
  claps, drum-kit Ogg, Ishizaka piano Ogg, Vivaldi soprano Ogg, Skidmore
  Beethoven Ogg), 1 redundant BY-licensed clap, 1 mono lossy bells file,
  1 uncertain-rights 1905 piano transfer, 1 too-short castanet hit.
  Full reasons in `manifest.json`.

## Source-quality and lossless-provenance assessment (2026-10-26)

Container format and demonstrated provenance are separate. `manifest.json`
v3 records four explicit per-candidate statuses (`source_quality_status`,
`lossless_provenance_status`, `b1_compatibility_status`,
`licensing_status`), per-candidate gate states (`gate_a_status`,
`gate_b_status`), and nullable legacy booleans (`lossless`,
`redistribution_permitted`, `conversion_permitted`: null while file bytes
or authorization are outstanding; see schema section). The table below
summarizes the evidence behind them. Nothing here upgrades any candidate:
all remain `needs-review`, and none counts as validated coverage.

| Fixture ID | Source quality | Lossless provenance | B1 compatibility |
|---|---|---|---|
| `ch-castanets-mono-48k` | Acceptable pending verification: native 48 kHz studio recording, documented chain (Tascam DR-40 + ME66) | Container verified from acquired bytes (PCM headers match listing); origin history still page-claims only | B1 input requirements verified (16-bit PCM accepted by `read_wav_i16`); scope decided at measurement |
| `ch-snare-mono-48k` | Acceptable pending verification: native 48 kHz studio recording, documented chain (MixPre-3 + KM184) | Container verified from acquired bytes (PCM headers match listing); origin history still page-claims only | B1 needs derived 16-bit file (acquired 24-bit rejected by `read_wav_i16`, verified); scope decided at measurement |
| `ch-piano-solo-stereo` | API-confirmed 44.1 kHz/16-bit Musopen studio recording; original ADC chain and pre-2019 re-encode history undocumented | Needs-review: FLAC container verified lossless via API; 2019 re-encode tag; lossy origin not excluded | Exploratory (native 44.1 kHz); no resampling to pass |
| `ch-bells-stereo-48k` | Acceptable pending verification: native 48 kHz/24-bit, documented MS chain | Needs-review: recordist FLAC; transcode history unverified | Gated path (48 kHz stereo) pending probe + hashes |
| `ch-ambience-stereo-48k` | Acceptable pending verification: 96 kHz original, documented ORTF/Schoeps chain; served file listed 48 kHz | Needs-review: served 48 kHz WAV vs stated 96 kHz original implies an undocumented uploader-side 96->48 conversion | Gated path pending confirmation that the served file is 48 kHz PCM |
| `ch-asym-stereo` | Acceptable with limitation: native 44.1 kHz/16-bit full-band recording as listed; content flag open | Needs-review: WAV listed; origin unverified | Exploratory (44.1 kHz); no resampling to chase the gate |
| `ch-speech-vctk-48k` | Verified on acquired bytes: 48 kHz/16-bit chamber distribution (STREAMINFO), genuine wideband spectra, exposed sibilant bursts; mic identity still unstated | Container verified (FLAC headers + clean decode); 96->48/24->16/silence-trim pipeline in publisher texts, converter/filter unspecified | B1 input verified (decoded 16-bit PCM accepted); dual-mono + excerpt deferred to assembly |
| `ch-voice-clean-48k` | Acceptable pending verification: native 48 kHz clean voice as listed | Needs-review: WAV listed; origin unverified | Gated path (48 kHz) pending channels/PCM confirmation |
| `ch-soprano-solo-flac` | Band-limited by era: 1934 shellac; API confirms the 192 kHz stereo container is a modern transfer, not historical fidelity | Needs-review: FLAC container verified lossless via API, but transfer chain undocumented and content era-limited | Not directly usable (192 kHz needs downsampling; rights blocked regardless) |
| `ch-orchestra-tutti-flac` | Acceptable pending verification: 48 kHz/24-bit Musopen studio lineage, sibling of verified m8; original ADC chain undocumented | Needs-review: FLAC container verified lossless via API; 2007 encode; lossy origin not excluded by page text | Gated path (48 kHz/24-bit) pending per-file probe |

## Acquisition checklists (Gate A, then Gate B)

**Gate A — pre-download authorization (no bytes yet).** F05A outcomes
recorded in `manifest.json`: **pass** for VCTK (audio-only), castanets,
snare, bells; **blocked** for tutti (licence basis); **pending** for
piano, ambience, soprano, asym, voice-clean (review not performed this
phase). To pass Gate A for a candidate, a reviewer must record:
the exact source file confirmed from a re-fetched page; licence, permitted
acquisition, and intended use verified from primary evidence; all rights,
consent, content, and redistribution blockers resolved; and any residual
uncertainties that remain. Blocked candidates cannot pass until their
blockers clear — never authorize to enable investigation. A Gate A pass
authorizes acquisition only; it claims nothing about file bytes, which is
Gate B's job.

**Gate B — post-download technical acceptance (only after Gate A passes).**
For an authorized file: download the exact file and record the URL; match
the published checksum; probe headers (container, codec, rate, channels, bit
depth, duration); inspect spectral bandwidth for codec cutoffs, brick-wall
resampling artifacts, or era-limited bandwidth (confirm genuinely full-band
content for the stated rate; reject bandwidth-limited sources; never
upsample to qualify); audition the full file and confirm content, category
intent, and segment start/end in samples and seconds; verify conversion
history and category fit; convert per README.md policy with the pinned
tool build (verify bit-exactness where applicable); record the final WAV
SHA-256 plus the exact command; file attribution/notice obligations;
reviewer sign-off. Reject or return to review on any contradiction.

**Gate B evidence (F05B, 2026-10-26).** Method (reproducible, stdlib-only):
`wave`/`struct` header + PCM read; SHA-256 + byte size; Hann-windowed
radix-2 FFT band energies (four 4096-sample windows plus onset-window
analysis; FFT/window code reused read-only from B1/B2 tooling), spectral
rolloff99, crest/DC/clip stats, onset-jump and peak-spread analysis; B1
`read_wav_i16` acceptance probed read-only (no B1 code changed, no
encoding run). No listening performed; content match established by
statistical signature against the recordist description. Spectral
inspection describes the acquired file only; it does not prove the
recording chain was lossless end-to-end. Acquired bytes live in the
session acquisition dir (outside the repo), preserved unmodified; no
trimming, resampling, or any other transform applied.

| File (destination) | Source URL | Bytes / SHA-256 | Observed properties | Gate B |
|---|---|---|---|---|
| `ch-castanets-mono-48k.wav` | `https://bigsoundbank.com/UPLOAD/bwf-en/0922.wav` (page WAV button) | 3,456,106 (= 3.4 MB listing) / `208f9edb…ee7272` | 48 kHz / 16-bit / mono PCM, 1,727,513 samples (35.99 s); no bext chunk (plain WAV). 814 samples >50% FS spread over all 36 s; crest 33.5 dB; DC ~0; one full-scale sample. Spectrum: 1–4 kHz 77%, 4–8 kHz 16%, 8–12 kHz 7%, content to 21.6 kHz, rolloff99 11.1 kHz — wideband, no brick wall. B1 `read_wav_i16` accepts. | **pass** |
| `ch-snare-mono-48k.wav` | `https://bigsoundbank.com/UPLOAD/bwf-en/2309.wav` (page WAV button) | 115,518 (= 113 KB listing) / `43cb91df…7718b6c` | 48 kHz / 24-bit / mono PCM, 38,148 samples (0.795 s; page rounds to 00:01). Single hit at t=0, sharp onset jumps, crest 22.2 dB; LF-heavy body (0–250 Hz 70% in onset window) with decaying HF rattle — recorded as character. B1 `read_wav_i16` rejects 24-bit (verified): B1 use needs the documented 24→16 shift producing a derived fixture; original untouched. | **pass** |
| `ch-bells-stereo-48k` | manifest download URL | Acquisition FAILED: endpoint returns the Freesound login page (HTTP 200, 17 KB HTML) instead of the 128.1 MB FLAC. No credentials, no workaround. | Not acquired; no Gate B performed. Gate A stands. | **not performed** |
| VCTK bundle + utterances | manifest DataShare URL (serves `DS_10283_3443.zip`; inner `VCTK-Corpus-0.92.zip`, 11,747,302,977 B) | Inner ZIP MD5 `8a6ba294…` EXACT match to publisher record. Utterances: p225_021 464,348 B / SHA-256 `92b75593…`, p225_003 387,806 B / SHA-256 `877a8327…` | FLAC STREAMINFO 48 kHz/16-bit/mono, 9.033 s + 7.588 s; mic1 selected (mic2 present); p225 clear of update.txt deletions. Decoded 16-bit PCM: crest ~18 dB, speech modulation with pauses; spectra decay smoothly to 24 kHz (0.2% above 16 kHz, no 8 kHz brick wall); 4–12 kHz bursts peak ~47% of fullband RMS in 14% of blocks — exposed sibilants confirmed. B1 `read_wav_i16` accepts. Converter/filter for 96→48 unspecified; mic identity unstated. | **pass** |

Full SHA-256 values: castanets
`208f9edb5090163d45c07a940cf4b2b59418a1a9b59b203d1839feebeeec7272`;
snare `43cb91dff1425aee797dd6687dec5be127a260d8b0daaf4b6a2747718b6c0574`;
p225_021 `92b75593a8ff681f880cb70f1e9bc71c9549891e95a09427fabd9e844ebb0f57`;
p225_003 `877a8327608bd37b82d125adca98bdf7c92c904dff8c58fb711d7d9be364dc99`.
Inner ZIP MD5 `8a6ba2946b36fcbef0212cad601f4bfa` (exact publisher match);
outer bundle SHA-256 `3a6873867098ff2fe3b699059274f317ea8f2415214a5058c3271dcc41bcd5`.
Unresolved limitations: no published checksums exist for the BigSoundBank
files (identity by size match + headers + content instead); recording-chain
origin remains page-claims only; the snare's dark tonal character and the
0.795-vs-00:01 rounding are recorded observations, not failures. VCTK:
converter/filter for 96→48 unspecified, mic identity unstated, no
listening performed; spectra describe the acquired utterances, not the
whole recording chain.

**Processing-policy clarification.** Lossy perceptual codecs (MP3, Vorbis,
AAC) and undocumented bandwidth-destroying processing are unacceptable,
always. Documented producer-side sample-rate conversion or bit-depth
reduction (VCTK's stated 96-to-48 kHz distribution; felix.blume's stated
96 kHz original served at 48 kHz) may be acceptable only when the history
is documented, the resulting band fits the intended category, and Gate B
spectral inspection confirms genuine content — the "lossy step" ban targets
perceptual coding and undocumented bandwidth destruction, not documented
linear conversion. No conversion is declared harmless: VCTK's downsampling
filter is undocumented, so Gate B must confirm genuine 48 kHz content.
Upsampling never restores bandwidth and is forbidden as qualification.

## Fixture preparation (F05C, proposed only)

Source originals verified byte-identical (hashes re-matched 2026-10-26)
and preserved untouched; nothing was placed in the frozen baseline
fixture directory. Proposed fixtures live as file paths in
`proposed-fixtures.json` (schema v1, `proposed-pending-final-review`):
- Castanets: source WAV used directly (already B1-compatible 16-bit PCM).
- Snare: 24-bit original → derived 16-bit WAV by arithmetic right shift
  (`>> 8`, truncation toward −inf, no dither, no resampling), residual in
  [0, 255] verified over all 38148 samples; peak and max-jump indices
  identical, jump ratio 256.0 — transient intact. No perceptual claim.
  Follows the m8 precedent (verified sample-by-sample shift).
- VCTK p225_021/p225_003 (mic1): FLAC → 16-bit PCM WAV decode via
  `ffmpeg -c:a pcm_s16le -map_metadata -1 -fflags +bitexact`
  (ffmpeg 9.0.2, binary hash identical to the m8 build). No resample,
  no filter, no trim. B1 `read_wav_i16` accepts all four proposed files.
- Bells: no file (acquisition failed); tutti: blocked; others pending.

## Listening suitability (outstanding — no listening performed)

This environment performed no human listening review, and automated
spectral analysis is not a substitute for it. The F05D-1 worksheet
(`listening-review-worksheet.md`) holds the per-fixture review blocks;
all decisions start at `pending`. Outstanding checklist for a
future reviewer with playback capability:
1. Content/category match: castanet strikes / snare hit / read English
   speech with sibilants as labelled; no mismatched content.
2. Obvious recording or decode defects: clicks, dropouts, clipping beyond
   the single full-scale samples recorded, decode artifacts.
3. Unwanted background content: room noise, voices, music, hum.
4. Suitability for the intended challenge (transient pre-echo; sibilant
   HF coding) and segment choice for the eventual excerpt.
5. Any unexpected differences between the source description and the
   actual audio (e.g. snare tonal body, VCTK mic character).

## Manifest schema (v3, design-only)

Top level: `schema_version` (3), `manifest_kind`, `name`, `corpus_status`,
`approved_fixture_count` (0), `lossless_policy`, `full_band_policy`,
`lossless_provenance_policy`, `b1_boundary_policy`, `gate_policy`,
`coverage_policy`, `candidates[]`, `rejected[]`,
`requires_separate_review[]`, `notes[]`.
Per candidate: `fixture_id`, `category`, `rationale`, `title`,
`title_creator`, `source_page`, `direct_asset_url` (+ note where
login-gated), `source_tech_as_listed`, `licence`, `licence_evidence`,
`attribution_required` (+ text), `redistribution_permitted`,
`conversion_permitted`, `composition_recording_split`,
`redistribution_status`, `eligibility_state`, `lossless` flag,
`source_quality_status`, `lossless_provenance_status`,
`b1_compatibility_status`, `licensing_status`, `gate_a_status`,
`gate_b_status`, `target_sample_rate_hz`, `target_channels`,
`desired_segment_s`, `technical_suitability`, `coverage_contribution`,
`decision`, `decision_rationale`. The five status dimensions plus the two
gate states are explicit and must not be conflated: source quality,
lossless provenance, B1 compatibility, licensing, Gate A, Gate B, and the
overall decision.
Legacy boolean semantics (all currently null except where noted, because no
file bytes have been examined and no authorization granted):
`lossless: null` = container claimed lossless by the source listing but
bytes unverified (becomes true only after Gate B header verification;
false if lossy); `redistribution_permitted: null` = not yet authorized via
Gate A (true = authorized; false = refused — currently only the
rights-blocked soprano); `conversion_permitted: null` = no conversion
authorized (conversions happen only after Gate B; licence-level adaptation
rights live in `licensing_status`, not here). Positive booleans never imply
approval; gate decisions and status fields are authoritative.
Future acquisition appends: Gate A record, acquired-byte checksum,
header-probe record, spectral-bandwidth verification, segment offsets,
conversion command, tool identity, final WAV SHA-256, reviewer sign-off,
and only then flips status.
