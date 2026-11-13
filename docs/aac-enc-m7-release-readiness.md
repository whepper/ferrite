# M7 — Multi-Profile Encoder Release Readiness Audit

Release-readiness audit milestone (VBR-1 through VBR-6). Validator-only
changes: one stale benchmark-pin correction, one comment cleanup, one
documentation-index update, rustfmt hygiene on five pre-existing
test-module violations, and one new regression test. No codec, bitstream,
oracle-output, or baseline changes.

**Verdict: `PASS WITH FINDINGS`**

All six profiles produce valid, independently decodable AAC-LC across the
production matrix; the Rust API and `aac-convert` agree on the VBR-6
default everywhere; measured spectral edges match the configured
bandwidths exactly; and no profile-specific signalling leaks into ADTS or
M4A. The findings are release-process items (untracked build-required
snapshots), environment-limited validation gaps (external C-reference and
FDK-decoder artifacts absent), and documentation/consistency items. No
critical or high-severity defect was found in the encoder, transports, or
public interfaces. No perceptual claim is made anywhere in this report.

---

## 1. Exact revision and repository state

| Item | Value |
|---|---|
| HEAD | `a1bcbada51a50ac0fcc92e84698807890ebae5e6` (v0.2.0, `main`, up to date with origin) |
| `git describe` | `v0.2.0-dirty` |
| Branch | `main` |
| Staged | empty |
| Tracked diff (at audit start) | 21 files, +2153/−159 (the uncommitted multi-profile implementation) |
| Untracked (at audit start) | `rust/docs/vbr-profiles.md`, `rust/examples/aac_bench_wav.rs`, 14 `cfg_*.txt` snapshots, `tools/enc-oracle/dump_vbr_init_configs.cpp` |

Audit-added changes (all uncommitted, nothing staged or committed):

| Change | Class |
|---|---|
| `rust/examples/aac_bench_wav.rs` — default-profile pins corrected to the VBR-6 stream (finding F-3) | fix |
| `rust/src/aac_enc/encoder.rs` — `Encoder::vbr_mode()` comment cleanup (F-5) | fix |
| `rust/src/aac_enc/production_baseline.rs` — new `production_default_profile_vbr6_music_baseline_is_unchanged` regression test (F-3) | fix + regression test |
| `docs/README.md` — VBR-5 index wording + link to the authoritative profile reference (F-6) | fix |
| `rust/src/aac_enc/{bitcount,m5a_decode,m5b_measure,m5g_census,quant}.rs` — rustfmt hygiene, test modules only (F-7) | fix |
| `rust/examples/m7_audit_sweep.rs`, `rust/examples/m7_audit_spectral.rs` — audit-only harnesses (untracked; see §7/§8) | audit tooling |

Every prior-milestone conclusion was re-verified against this exact tree;
no earlier result was assumed.

## 2. Supported configuration matrix

| Dimension | Config plane (`EncoderConfig`) | Production plane (`Encoder`) |
|---|---|---|
| Object type | AAC-LC only (`AOT_AAC_LC = 2`) | AAC-LC only |
| Profiles | VBR-1 … VBR-6 (`VbrMode`, discriminants = C enum values) | VBR-1 … VBR-6; default VBR-6 |
| Sample rates | 12 LC rates, 8 000–96 000 Hz | 44 100 / 48 000 Hz only |
| Channels | mono … 8 (channel-map table; LFE excluded from effective count) | mono / stereo only |
| Frame length | 1024 only (960 passes `AdjustEncSettings` but is rejected for LC) | 1024 |
| Transports | `Raw`, `Adts` | ADTS only |
| CBR | none (user bitrate ignored, as in C VBR path) | none |
| SBR / PS / HE-AAC | none | none |
| Encoder delay | 1600 samples per channel (`DELAY_AAC(1024)`) | same, used for gapless accounting |

The planes are intentionally different and documented
(`encoder-integration.md` "Known limitations"); construction outside the
production plane returns `EncodeError::UnsupportedConfiguration`.

## 3. Profile configuration audit (VBR-1 … VBR-6)

Static verification (all confirmed in source and pinned tests):

- Central definition: `VbrProfile::for_mode` carries per-profile mono /
  stereo-per-channel bitrates, bandwidth, float + Q31 quality factor, and
  origin (`FDK` × 5, `Ferrite` × 1). VBR-6 inherits every VBR-5 value
  except bandwidth (20 000 Hz vs 19 293 Hz) — enforced by
  `vbr6_inherits_vbr5_except_bandwidth`.
- Snapshot selection: `load_cfg_for_profile` keys embedded
  `include_str!` snapshots by `(sample_rate, per-channel bitrate)`; all
  18 production snapshots resolve (test `every_profile_resolves_to_exact_snapshot`);
  unknown combinations panic fail-closed.
- `peOffset`: `vbr_pe_offset` reproduces the exact C formula; 50 only at
  VBR-1 stereo (20 kbit/s per channel), 0 elsewhere
  (`vbr_pe_offset_matches_c_reference`).
- `ahParam`: `modifyMinSnr=1`, `startSfbL/S = 15/3` for every profile
  (all per-channel rates ≥ 20 kbit/s, so the disabling branch never
  fires) — pinned in the parity test.
- Quality factor: per-profile Q31 literals consumed by
  `AtsElement::for_vbr_mode` → `reduceThresholdsVBR`.
- TNS: derived through the shared `TnsConfig::new`
  (`stopBand = min(maxBands, sfbActive)`), pinned per operating point.
- M/S: always allowed; intensity: `allowIS = (bitrate / bandwidth) < 5`
  (VBR-1 stereo only), matching C.
- PNS: off for every profile (`NO_NOISE_PNS` sentinels; effective FDK
  VBR behaviour, not the initial AAC-LC default).
- Afterburner: on for every profile (`invQuant = 2`).
- QC init: `derive_qc_init` consumes the nominal total bitrate,
  effective bandwidth and frame length; `averageBits`, `maxBits`,
  `minBits`, `bitRes` (FULL mode), `meanPe`, `peMin`, `peMax` all pinned
  against real-C `QCInit` outputs for 20 operating points
  (`vbr_profiles_match_c_reference_init`, VBR-1..5 × 44.1/48 kHz ×
  mono/stereo) — **PASS** in this tree.
- Frame-bit limits / reservoir: `maxBits` (6144 mono / 12 288 stereo
  bits), `minBits = 0`, `bitRes = 6144·nEff` pinned per operating point.
- ADTS signalling: profile field is AAC-LC for every profile; VBR
  fullness `0x7FF`; nothing profile-specific reaches the header.
- Default selection is VBR-6 in every default path:
  `EncoderConfig::new`, `stereo_48k_adts`, `StreamEncoder::new`,
  `AtsElement::new`, all `convert_file*` wrappers, `BatchOptions`
  default, and the CLI parser. No default path selects anything else.

**Reported vs consumed values:** no discrepancy found. `nominal_bitrate`
(per-channel × effective channels) equals the CLI-reported nominal and
the rate-control input for every constructible production configuration;
`effective_bandwidth_hz` equals the value passed to SFB/QC
(`Nyquist` clamp inactive at 44.1/48 kHz, binding below — pinned by
`effective_bandwidth_respects_sample_rate`).

Dynamic verification: the 192-cell sweep (§7) encodes every profile at
both rates and channel counts through the public API with per-field ADTS
header validation; every cell constructs, encodes and validates.

**Verdict: PASS.**

## 4. ADTS validation results

Every ADTS frame of every sweep cell (192 synthetic + 24 production
fixtures = 216 streams, 18 508 frames per profile) was validated
field-by-field by the audit harness:

- sync word `0xFFF`; MPEG-4 ID; layer 0; protection-absent (no CRC)
- profile = AAC-LC (field value 1) for all six profiles
- sample-rate index correct (4 = 44.1 kHz, 3 = 48 kHz)
- channel configuration correct (1 mono, 2 stereo)
- private/original/home/copyright bits zero; single raw data block
- 13-bit frame length includes the 7-byte header; frame walk consumes
  the stream exactly (no trailing bytes, no gaps)
- buffer fullness `0x7FF` on every frame (VBR signalling; C path clamps
  `FDK_INT_MAX`)
- frame indexes strictly sequential from `push` through `flush`

Result: **PASS** — zero violations across all 216 streams; no
profile-dependent signalling; no SBR/PS indicators anywhere.

Encoder delay / termination: flush emits the padded partial frame plus
delay frames exactly once; the frame law
`ceil((samples + 1600) / 1024)` held for every cell (verified against
decoded counts, §6).

## 5. M4A/MP4 validation results

Via the real `aac-convert` CLI (release build of this tree), mono 44.1 kHz
and stereo 48 kHz fixtures × {default, `--vbr 1..6`, `--vbr=N`}:

- `esds`: `objectTypeIndication = 0x40` (MPEG-4 audio),
  `bufferSizeDB = 0`, `maxBitrate = avgBitrate = 0` (declared unknown —
  the muxer does **not** present the nominal bitrate as measured, per the
  code contract).
- `DecoderSpecificInfo` = exactly `EncoderConfig::audio_specific_config()`:
  `0x11 0x90` (AAC-LC, 48 kHz, stereo), `0x12 0x08` (AAC-LC, 44.1 kHz,
  mono) — byte-verified by box parsing.
- `mp4a` sample entry: channelcount/samplesize(16)/samplerate(16.16
  fixed) correct for both channel layouts.
- Sample table: one sample per AAC access unit, sequential
  (`M4aWriter::write_frame` enforces frame-index continuity and rejects
  non-sequential writes), durations 1024, timescale = source rate.
- `ffprobe` on every file: `codec_name=aac`, `profile=LC`, correct
  rate/channels, `nb_frames` 14 (stereo) / 5 (mono), no errors.
- Gapless: `iTunSMPB` present on the default path; CoreAudio playback
  length trims to the exact source frame count (12 000 stereo / 3 000
  mono) while the container carries the full 14×1024 / 5×1024 — the
  documented delay/padding model. The `edit_list` switch stays optional
  and off by default.
- Profile independence: VBR-5 and VBR-6 files differ only in payload
  bytes (1959 vs 1961 total bytes on the stereo fixture); metadata
  (`©too = Ferrite 0.2.0`, tags, loudness atoms) identical across
  profiles; no profile-specific container metadata exists.

Result: **PASS** (ADTS + M4A), for mono and stereo.

## 6. Independent decoder compatibility

Decoders used (both independent of Ferrite; none added as a dependency):

| Decoder | Version | Result |
|---|---|---|
| FFmpeg / libavcodec | 9.0.2 | 216/216 ADTS streams decode, exit 0, **empty stderr** |
| Apple CoreAudio (`afconvert`/`afinfo`) | macOS 15 arm64 | 216/216 decode, packet/frame counts exact |

Checks per stream: successful decode; zero decoder errors or warnings;
decoded sample count exactly `frames × 1024` per channel (delay +
padding accounted); PCM peak sampled — no clipping (max observed peak
≈ 0.97 FS on the loudest synthetic cell, silence stays digital-zero);
no NaN/Inf possible in s16 PCM; channel layout reported correctly by
both decoders.

M4A: 16 files (2 fixtures × 8 profile selections) decode through both
decoders; FFmpeg reports the full container length, CoreAudio applies
`iTunSMPB` and reports the trimmed source length — expected,
decoder-specific gapless behaviour, not an inconsistency.

FDK-AAC decoder: **NOT VALIDATED** in this environment — no prebuilt
`aac-dec` (`AAC_DEC_BIN` unset, no local FDK build tree); the
`fdkaac` binary present on PATH is the encoder frontend only. This is
reported as a gap, not a pass (§11).

Result: **PASS** on ffmpeg + CoreAudio; FDK decoder NOT VALIDATED.

## 7. Actual bitrate behaviour (measured)

Sweep harness: public `Encoder` API, release build, deterministic
synthetic content (8 kinds: speech, sparse, dense, transient, sustained
tonal, silence, near-silence, high-frequency) × 6 profiles ×
{44.1, 48} kHz × {mono, stereo}, 8–12 s per cell, plus the four
committed production WAV fixtures. 216 streams; full table in the audit
log; representative rows (stereo 48 kHz, bytes = ADTS stream size):

| Content | VBR-1 | VBR-2 | VBR-3 | VBR-4 | VBR-5 | VBR-6 | nominal |
|---|---:|---:|---:|---:|---:|---:|---:|
| music fixture (m7-music) | 93.0 k | 89.6 k | 100.6 k | 128.5 k | 199.8 k | 204.4 k | 192 k |
| orchestral (m8-eroica) | 92.9 k | 90.4 k | 101.8 k | 127.1 k | 186.5 k | 191.9 k | 192 k |
| speech clean | 63.5 k | 63.1 k | 69.5 k | 80.6 k | 101.8 k | 103.0 k | 192 k |
| speech pauses | 63.0 k | 62.6 k | 67.8 k | 79.1 k | 100.2 k | 101.4 k | 192 k |
| dense synthetic | 29.5 k | 28.4 k | 35.4 k | 44.8 k | 70.7 k | 71.4 k | 192 k |
| transient synthetic | 27.9 k | 27.4 k | 20.8 k* | 27.4 k | 79.4 k | 82.0 k | 192 k |
| silence | 5.6 k | 5.6 k | 5.6 k | 5.6 k | 5.6 k | 5.6 k | 192 k |

(*the VBR-3 anomaly is a property of that synthetic click train, not a
rate-control fault: it sits between VBR-2 and VBR-4 on all other
content; see limitations.)

Frame-size behaviour (bytes incl. 7-byte header):

| Metric | VBR-1 | VBR-2 | VBR-3 | VBR-4 | VBR-5 | VBR-6 |
|---|---:|---:|---:|---:|---:|---:|
| global min frame | 13 | 13 | 13 | 13 | 13 | 13 |
| global max frame | 496 | 490 | 593 | 719 | 966 | 1016 |
| frames ≥ 8186 B (13-bit cap) | 0 | 0 | 0 | 0 | 0 | 0 |
| stereo `maxBits` headroom | 1536 B cap never approached | | | | | |

- Silence produces constant minimal frames (13/15 B) on every profile —
  no pathological growth, no reservoir runaway.
- Initial frames are minimal (13/15 B) or small on all profiles; final
  flush frames stay far below limits.
- VBR-5 vs VBR-6 differ only via the bandwidth path: +1.9 % average on
  music, +2 bytes on the 14-frame fixture — consistent with more active
  high-frequency bands, nothing else.
- No frame in 111 048 encoded frames approached the ADTS 13-bit limit or
  the per-frame `maxBits` cap; no retry-storm or crash-marker behaviour
  appeared (crash paths separately covered by 7 passing crash tests).
- Reservoir: VBR runs `bitResMode = FULL` with `minBits = 0`
  (pinned per operating point); the per-frame `totalAvail == maxBits`
  invariant from the M4-S6 work still holds (lib suite green).

C-reference output distributions: **NOT VALIDATED** here
(`PHASE1_C_DIR` / `M55_C_DIR` unset — §11). The committed byte/hash pins
(VBR-5 corpus; new VBR-6 default pin) remain the C-anchored evidence.

Result: **PASS WITH FINDINGS** (F-8 documentation nuance: the docs'
nominal-bitrate framing is correct, but the CLI example line ordering
"[.., nominal 192000 bit/s]" can be misread as measured — docs already
label it "nominal"; kept under observation, no change).

## 8. Bandwidth and spectral-output validation

Method: per profile, one stereo file of consecutive 0.5 s pure-sine
segments (10/15/17/18/19/19.5/19.8/20.2 kHz) encoded through the public
API, decoded with ffmpeg (zero stderr), analysed with Goertzel at each
synthesis frequency, relative to the 10 kHz segment. Run at 48 kHz (all
profiles) and 44.1 kHz (VBR-2/5/6).

Measured segment level (48 kHz, dB rel. 10 kHz reference; "gone" ≈
−96 … −135 dB, i.e. band not coded):

| Tone | VBR-1 (13 k) | VBR-2 (13 k) | VBR-3 (15.75 k) | VBR-4 (16.5 k) | VBR-5 (19.293 k) | VBR-6 (20 k) |
|---|---:|---:|---:|---:|---:|---:|
| 15 kHz | gone | gone | +0.2 | +0.2 | −0.0 | −0.0 |
| 17 kHz | gone | gone | gone | gone | −0.0 | −0.0 |
| 18 kHz | gone | gone | gone | gone | +0.0 | +0.0 |
| 19 kHz | gone | gone | gone | gone | +0.0 | +0.0 |
| 19.5 kHz | gone | gone | gone | gone | −100.9 | **+0.0** |
| 19.8 kHz | gone | gone | gone | gone | −108.7 | **+0.0** |
| 20.2 kHz | gone | gone | gone | gone | −117.0 | −96.1 |

Every profile's measured spectral edge lands exactly at its configured
bandwidth; 44.1 kHz results are identical (Nyquist 22 050 Hz inactive
for all profiles, as configured). The sharpest contrast: 19.5/19.8 kHz
tones survive at full level under VBR-6 and are removed under VBR-5 —
the VBR-6 bandwidth extension is real in the decoded output, and VBR-5
is unchanged from its configuration. For pure stationary tones the
low-pass line behaves as a hard band exclusion above the configured
bandwidth; the docs' "not a guaranteed spectral cutoff" caveat remains
the right description for broadband content. No aliasing, discontinuity
or pathological high-frequency behaviour appeared in any decoded
sample.

No perceptual claim is made or implied (VBR-6 remains a Ferrite
extension; §12 limitations).

Result: **PASS.**

## 9. Public Rust API audit

- Default: `EncoderConfig::new` / `stereo_48k_adts` → VBR-6 (unit
  tests + doc examples). Explicit `VbrMode::Vbr1..Vbr6` via
  `new_with_mode` / `with_vbr_mode` (all six exercised by tests).
- Public accessors: `vbr_mode()`, `profile()` (full definition incl.
  origin), `nominal_bitrate()`, `channel_bitrate()`,
  `vbr_qual_factor()/vbr_qual_q31()`, `configured_bandwidth_hz()`,
  `effective_bandwidth_hz()`, `encoder_delay()`,
  `audio_specific_config()` — all profile-correct and documented.
- Invalid configuration: each `ConfigError` variant tested with exact
  `Display` strings; production-plane rejection surfaces as
  `EncodeError::UnsupportedConfiguration`.
- Mono/stereo handled symmetrically (mono uses the mono table entry).
- Doc examples compile (19 doc tests) and match implementation
  behaviour (doc-test output verified in-suite).
- No API surface assumes VBR-5: production code has no VBR-5 constants
  outside `config.rs`/profile definitions; `m6_migration.rs` and
  `production_baseline.rs` pin VBR-5 *explicitly* via `with_mode`, so
  the default change cannot silently move those baselines.
- Error messages and enums unchanged from M6 (no breaking changes).

Comment-only cleanup applied to `Encoder::vbr_mode()` (F-5). 

Result: **PASS** (after F-5).

## 10. `aac-convert` CLI audit

Verified against the release build of this tree:

| Check | Result |
|---|---|
| `--vbr` defaults to 6 | PASS (diag line `VBR-6`; byte-identical to explicit `--vbr 6`) |
| `--vbr 1..6` each work | PASS (all six, mono + stereo, M4A + ADTS) |
| `--vbr=N` form | PASS (`--vbr=3`, `--vbr=4`) |
| Aliases (`VBR-3`, `vbr2`) | PASS (documented behaviour) |
| Invalid values (`0`, `7`, `abc`, empty) | PASS — exit 2, `error: invalid --vbr value: … (expected 1, 2, 3, 4, 5, or 6)` |
| Help documents default | PASS (`(default: 6)` in usage text) |
| Single-file conversion | PASS |
| Batch conversion incl. recursive dir | PASS (header names profile: `format M4A, VBR-4, recursive, 2 jobs`) |
| Mono and stereo | PASS |
| ADTS and M4A output | PASS |
| Diagnostics report profile + nominal + bandwidth | PASS (`…, VBR-6, nominal 192000 bit/s, 20000 Hz bandwidth`) |
| Nominal not presented as measured | PASS (word "nominal"; muxer declares 0/0) |
| Existing options compatible | PASS (`--format`, `--no-tags`, `--cover-art`, `--jobs`, `--dry-run`, `--skip-existing`, `-v/-q` — full cli/batch suites green) |
| Overwrite protection | PASS (re-conversion to an existing output fails; behaviour unchanged) |

Result: **PASS.**

## 11. Regression and stability testing

Full workspace (`cargo test --workspace --no-fail-fast`, this tree):

| Target | Result |
|---|---|
| `ferrite-aac` lib (362 tests) | **350 passed**, 11 not-executable (below), 1 ignored (opt-in perf bench) |
| `ferrite-aac` production_api integration | 6 passed |
| doc tests | 19 passed |
| `aac-convert` lib unit | 80 passed |
| `aac-convert` batch / cli / conversion / coverart / loudness / metadata | 62 / 18 / 30 / 35 (1 ignored, ffprobe-run separately: pass) / 16 / 22 (1 ignored, run separately: pass) |
| `aac-convert` independent_decode | 2 passed (run explicitly with `--ignored`; ffmpeg present) |

Tests that could **not execute** (missing external artifacts; all fail
loudly with explicit `reference validation artifact missing` messages —
not silently skipped):

| Test(s) | Missing artifact | Why needed |
|---|---|---|
| 9 × `phase1_tests::*` | `PHASE1_ORACLE_DIR` + `PHASE1_C_DIR` | frame-by-frame C oracle parity |
| `m55_streams_match_c_reference` | `M55_C_DIR` | full-stream C-reference comparison |
| `m5a_decode_check` | `AAC_DEC_BIN` | FDK decoder round-trip |

The suite is therefore **not** reported as fully green: the default run
passes 636 tests (350 encoder lib + 6 production API + 19 doc + 261
converter), and the 4 design-ignored tests that require ffmpeg run
separately and pass (2 independent-decode, 1 cover-art ffprobe, 1
metadata ffprobe). 11 encoder-lib tests could not execute for
environmental reasons; 1 is an opt-in benchmark, ignored by design.
No test was weakened and no fixture was altered to achieve this result.

Other gates (all PASS): `check-licensing.sh` (67 source files carry the
modified-version notice), `cargo fmt --all -- --check`, `git diff
--check`, `cargo build --no-default-features --features aac_enc`, crash
tests (7), C-reference init parity (20 pinned operating points),
M5/M6 production baselines (19 VBR-5 fixtures, byte + hash),
VBR-6 default output (new pin, §13), `cargo run --release --example
aac_bench_wav` assertions after the pin fix.

`cargo package --locked -p ferrite-aac` **cannot complete** on this
tree by design (Cargo refuses dirty worktrees) — see F-1.

Clippy (informational, per CI): no errors; the existing warning set is
unchanged upstream-mirroring style lints.

## 12. Documentation review

Reviewed against this tree: main `README.md`, `rust/README.md`,
`rust/docs/vbr-profiles.md`, `rust/docs/encoder-integration.md`,
`rust/docs/user-manual.md`, `rust/convert/README.md`,
`rust/docs/aac-convert-integration-contract.md`, `docs/README.md`,
release notes. The working tree's documentation already states: six
profiles; VBR-6 default; VBR-5 = upstream FDK baseline; VBR-6 differs
from VBR-5 by bandwidth only; AAC-LC only; no HE-AAC/SBR/PS; afterburner
intentionally on; PNS off; actual bitrate content-dependent; configured
bandwidth not a guaranteed cutoff; no perceptual claims.

Documentation changes made by this audit:

- `docs/README.md`: the VBR-5 entry no longer calls VBR-5 "the fixed
  encoder operating point" (it is the upstream baseline among six), and
  the index now links `rust/docs/vbr-profiles.md` as the authoritative
  profile reference (F-6).

Intentionally **not** changed: `rust/docs/releases/0.2.0.md` describes
the tagged v0.2.0 (which was VBR-5-only) and is historical record;
rewriting it would falsify release history (F-4, informational). The
dated `docs/aac-enc-vbr5-profile.md` is a VBR-5-specific audit record —
correct as written.

## 13. Findings by severity

No Critical findings. No High findings.

**F-1 (High, release process — outside this milestone's allowed
operations):** the fourteen VBR-1..4 configuration snapshots
(`rust/src/aac_enc/testdata/cfg_{44100,48000}_{20000,32000,40000,48000,56000,64000,72000}.txt`)
are `include_str!`-required at build time but **untracked**. Any clean
checkout of this state fails to compile the encoder (the M6.3 packaging
mechanism makes a missing snapshot a compile-time error), and
`cargo package` additionally refuses the dirty tree. Remediation
(matching M6.3's precedent): `git add` the fourteen files (plus the
other untracked implementation files) when the milestone work is
committed. Not performed here — committing is prohibited.

**F-2 (Medium, validation gap):** FDK-AAC decoder round-trip
(`m5a_decode_check`), phase-1 oracle parity, and M5.2 C-reference
stream comparison could not execute (`AAC_DEC_BIN`, `PHASE1_*`,
`M55_C_DIR` absent). Bitrate distributions could not be compared
against live C output. Compensating evidence in this audit: two
independent decoders × 216 streams + full byte/hash pins + 20-point
init parity. The affected tests remain mandatory for the next
environment that has the artifacts; the report does not claim their
coverage.

**F-3 (Medium, stale benchmark pins — fixed):**
`examples/aac_bench_wav.rs` pinned the VBR-5 stream (250 926 bytes,
`0x151e37b9…`) as the *default*-profile expectation while
`EncoderConfig::new` now defaults to VBR-6 (256 763 bytes,
`0xd6fc9d2c…`); the documented example command would panic on its own
assertion. Fixed: pins corrected to the VBR-6 default streams, comment
states the profile, and a new regression test
(`production_default_profile_vbr6_music_baseline_is_unchanged`) pins
the default-profile output bytes (all platforms) and hash (reference
platform) so any future default-profile change is an explicit test
update. Example re-run: assertions pass; new test passes.

**F-4 (Informational):** `rust/docs/releases/0.2.0.md` still says
"fixed VBR-5 profile" — correct for the tagged v0.2.0, stale relative
to this tree. Action: the multi-profile work needs its own release
notes (and version decision) at commit time; release history itself
must not be edited.

**F-5 (Low, code comment — fixed):** `Encoder::vbr_mode()` carried a
thinking-out-loud comment ("keep a cheap accessor via feeder? …").
Replaced with a factual one; behaviour unchanged; module tests re-run.

**F-6 (Low, documentation index — fixed):** `docs/README.md` described
VBR-5 as "fixed encoder operating point" and omitted the authoritative
`rust/docs/vbr-profiles.md`. Corrected (see §12).

**F-7 (Low, hygiene — fixed):** five files
(`bitcount.rs`, `m5a_decode.rs`, `m5b_measure.rs`, `m5g_census.rs`,
`quant.rs`) carried pre-existing `cargo fmt --check` violations (test
modules only, present at HEAD under the pinned 1.99.1/rustfmt 1.10.0
toolchain). Reformatting applied; lib suite re-run green; no
production-line changes (`git diff` confirms test-module-only hunks).

**F-8 (Informational):** the synthetic VBR-3 transient cell encodes
below VBR-2 on one click-train stimulus (20.8 vs 27.4 kbit/s). VBR-3
behaves monotonically on all other content and matches C at init; the
perceptual/bitrates-on-natural-material question is exactly the class
of claim this milestone defers to quality evaluation. Recorded, not
actioned.

**F-9 (Informational):** `esds` declares `maxBitrate = avgBitrate = 0`.
This is the honest "unknown" encoding (the encoder API exposes no
measured bitrate) and the docs say so; players ignore it. No change.

## 14. Outstanding limitations

1. External C-reference and FDK-decoder validations (F-2) remain
   not executed in this environment; they are required before any
   future distribution claim that cites them.
2. Perceptual quality is explicitly out of scope: no listening, ABX, or
   metric-based claim is made for any profile, and none may be inferred
   from this report. VBR-6's 20 kHz bandwidth is validated as
   configuration + decoded-spectral behaviour only.
3. The `minSnr`/spreading snapshots cover 44.1/48 kHz production rates
   only; other config-plane rates cannot reach the production encoder
   (fail-closed panic by design).
4. The VBR-3 synthetic-transient observation (F-8) is unexplained at
   the perceptual level and folded into the future quality-evaluation
   effort.
5. The multi-profile work (implementation + these audit changes) is
   uncommitted by milestone constraint; F-1 must be actioned at commit
   time.

## 15. Overall release-readiness verdict

**PASS WITH FINDINGS.**

- All six profiles validated end-to-end (API → encoder → ADTS → M4A →
  two independent decoders): **PASS**.
- Rust API and CLI defaults agree on VBR-6 everywhere: **PASS**.
- All profiles produce valid AAC-LC streams; no profile-specific
  signalling: **PASS**.
- ADTS and M4A independently validated (ffmpeg + CoreAudio): **PASS**
  (FDK decoder: NOT VALIDATED, environmental).
- Bitrate behaviour measured and documented across 216 streams /
  111 048 frames: **PASS WITH FINDINGS** (C-reference distribution
  comparison not executable).
- VBR-5 and VBR-6 regressions pinned and green: **PASS**.
- Documentation matches implementation: **PASS** (after F-6; F-4
  tracked for release time).
- Remaining limitations explicit and actionable: **PASS** (§14).

Release-blocking actions before distribution: commit the tree including
the fourteen snapshot files (F-1); execute the artifact-gated tests in
an environment that has the C reference and FDK decoder (F-2); prepare
post-0.2.0 release notes (F-4).

*Audit tooling used for the measurements in §7/§8 is retained
untracked as `rust/examples/m7_audit_sweep.rs` and
`rust/examples/m7_audit_spectral.rs` (not wired into CI); the decode,
CoreAudio, CLI, and spectral analysis scripts plus raw outputs were
kept outside the repository in the session workspace.*
