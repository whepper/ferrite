# AAC-LC VBR-5 Encoder Profile: C Reference and Rust Implementation

Status: source-audit and documentation record. No encoder behaviour was changed.

- Date: 2026-10-22
- Repository HEAD: `e51f35fe3d4e757a2a140455eb69c1e8d770dab9`
- C reference: FDK AAC v3.0.0 (`99356b5`) with the repository's encoder strip
  commits applied; the algorithm files listed in §1.3 are unchanged from
  v3.0.0.
- Rust target: `rust/src/aac_enc/`, production path `Encoder` →
  `StreamEncoder` → frame writer.
- Companion specification: [`aac-enc-migration-spec.md`](./aac-enc-migration-spec.md)
  (especially §3 "VBR 5 analysis" and §5 "Numerical compatibility requirements").
  This document does not repeat the full migration plan; it describes the
  profile and audits C/Rust correspondence.

---

## 1. Purpose and scope

### 1.1 What this document establishes

This document answers four questions from source evidence:

1. What does the public "VBR mode 5" selection actually configure in the C
   encoder?
2. Which of those values are fixed by the preset, which are inherited from the
   AAC-LC defaults, and which are left to per-frame runtime decisions?
3. How does the Rust encoder represent the same profile, and where does its
   implementation differ from the C reference?
4. What can and cannot be claimed about bitrate and quality from the existing
   evidence?

Every significant claim cites a file and line range. Values are given with
units. Where the source does not establish a value, the document says
`Not established` rather than inferring it from a name.

### 1.2 What this document does not establish

- It does not claim perceptual transparency, "CD quality", or equivalence to
  any listening reference. No ABX or listening campaign was run or is reported
  here.
- It does not define a target bitrate. VBR-5 has a nominal rate table, not a
  bitrate objective (see §6).
- It does not claim that the Rust encoder is bit-exact with C end to end. The
  Rust front end is float-native in several stages; known divergence classes
  are listed in §7 and §8.
- It does not re-validate the historical measurements. It cites them and marks
  their scope.

### 1.3 Method and evidence classes

The audit was read-only. Three evidence classes are used:

- **[Source]** a value or branch read directly from the cited C or Rust source.
- **[Measured]** a number taken from a committed or retained measurement record,
  with the record cited.
- **[Interpretation]** an engineering explanation that follows from the source
  but is not itself a source constant or a measurement.

The C tree is a stripped derivative of FDK AAC v3.0.0. The following files are
byte-identical to the untouched v3.0.0 tag (`git diff 99356b5 -- <paths>` is
empty): `qc_main.cpp`, `adj_thr.cpp`, `bandwidth.cpp`, `psy_configuration.cpp`,
`psy_main.cpp`, `ms_stereo.cpp`, `intensity.cpp`, `sf_estim.cpp`,
`quantize.cpp`, `aacenc_tns.cpp`, `block_switch.cpp`. Among the files cited in
this document, only `aacenc.cpp` (one line: forced `invQuant = 2`),
`aacenc.h` (removed SBR include), and the API/configuration layer
`aacenc_lib.cpp` differ; the strip additionally deleted the SBR, MPS and
metadata modules. This matters: the VBR-5 *algorithm* is upstream FDK v3.0.0;
the stripped tree changes which configuration can reach it.

---

## 2. Profile identity and terminology

### 2.1 Names and values

| Layer | Name | Value | Source |
|---|---|---:|---|
| Public API parameter | `AACENC_BITRATEMODE` | `0x0102` | `cpp/libAACenc/include/aacenc_lib.h:1287` |
| Public API value | VBR mode 5 ("very high bitrate") | `5` | `cpp/libAACenc/include/aacenc_lib.h:1296-1305` |
| Internal encoder enum | `AACENC_BR_MODE_VBR_5` | `5` | `cpp/libAACenc/src/aacenc.h:189-197` |
| QC kernel enum | `QCDATA_BR_MODE_VBR_5` | `5` | `cpp/libAACenc/src/qc_data.h:114-124` |
| Rust constant | `BITRATE_MODE_VBR_5` | `5` | `rust/src/aac_enc/config.rs:109` |

`AACENC_BR_MODE_IS_VBR(mode)` is true for all modes 1 through 5
(`aacenc.h:201`). VBR-5 is not implemented as a separate algorithm; it is a
row in three tables (nominal rate, bandwidth, quality factor) plus the shared
VBR branches. Beyond the enum definitions above, the literal `VBR_5` appears in
the encoder algorithm modules only here:

- `aacenc.cpp:205` — nominal per-channel bitrate row;
- `aacenc.cpp:230,307,702-703` — switch mappings;
- `bandwidth.cpp:158,273` — effective bandwidth row;
- `qc_main.cpp:131-132,1182,1256` — quality factor and VBR fill/reservoir
  branches.

This is an important negative result: there is no VBR-5-specific psychoacoustic
model, no VBR-5-specific block-switch rule, and no VBR-5-specific stereo rule.
Those tools are configured by the common AAC-LC path and then driven by the
signal.

### 2.2 The reference is a stripped build

The C reference in this repository is not a generic FDK AAC build. Commit
`8ccbe32` ("Strip encoder to AAC-LC VBR-5 afterburner-on, keep decoder")
changed the parameter plane as follows:

- only `AOT_AAC_LC` is accepted (`aacenc_lib.cpp:557-560`);
- only `AACENC_BR_MODE_VBR_5` is accepted (`aacenc_lib.cpp:561-563`);
- afterburner is forced on (`aacenc_lib.cpp:564-565,574`;
  `aacenc.cpp:716`);
- PNS is forced off (`aacenc_lib.cpp:665-666`);
- user bitrate is ignored and replaced by the VBR-5 table value
  (`aacenc_lib.cpp:619-621`);
- SBR, MPS and ELDv2 support is removed or rejected, LATM/LOAS/ADIF are
  rejected, and metadata is disabled
  (`aacenc_lib.cpp:553-607,678-679,1480-1485`).

Two of these are **not** properties of upstream FDK VBR-5:

| Setting | Upstream v3.0.0 default | This repository's reference |
|---|---|---|
| Afterburner (`useRequant` / `invQuant`) | off (`useRequant = 0` after `FDKmemclear`; `aacenc.cpp:337-368`; `aacenc_lib.cpp:660-680` in `99356b5`) | on, `invQuant = 2` (`aacenc.cpp:716`) |
| PNS (`usePns`) | on, `1` (`aacenc.cpp:353-354`; `aacenc_lib.cpp:678` in `99356b5`) | off, `0` (`aacenc_lib.cpp:665-666`) |

Consequently, the acceptance target documented here is precisely:

> **AAC-LC + VBR mode 5 + afterburner on + PNS off + ADTS transport**
> (the C build also accepts RAW).

It is not "any upstream FDK VBR-5 configuration". A Rust implementation that
matched only mode number 5 but used afterburner off or PNS on would not match
this reference. Conversely, a future upstream comparison must state which
afterburner/PNS setting was used.

### 2.3 Terminology

- **AAC-LC**: MPEG-4 Advanced Audio Coding, Low Complexity; AOT 2.
- **VBR**: variable bitrate. The encoder chooses frame content and size from
  signal requirements; it does not target a fixed number of bits per frame.
- **Nominal bitrate**: the table value used to initialize rate-control state.
  It is not a promise about output size.
- **PE**: perceptual entropy, an internal loudness/bit-demand measure.
- **SFB**: scalefactor band, the frequency partition used for thresholds,
  scalefactors and Huffman sections.
- **MDCT**: modified discrete cosine transform; 1024 lines per long frame,
  8 × 128 lines per short frame.
- **TNS**: temporal noise shaping, an LPC filter applied in the frequency
  domain to shape quantization noise over time.
- **M/S**: mid/side stereo; per-band sum/difference coding.
- **IS**: intensity stereo; the right channel is represented by an intensity
  direction and scale instead of spectrum in correlated high bands.
- **PNS**: perceptual noise substitution; noise-like bands are signalled by
  energy instead of spectrum.
- **Afterburner**: FDK's analysis-by-synthesis scalefactor refinement
  (`invQuant = 2`), not a separate bitrate mode.
- **Bit reservoir**: a buffer that lets CBR frames borrow bits across frame
  boundaries. VBR-5 pins it but does not use it to carry bits (§6.2).

---

## 3. Configuration flow

### 3.1 From the public parameter to the internal mode

1. The caller sets `AACENC_BITRATEMODE = 5`
   (`aacenc_lib.cpp:1296-1305`). In the stripped build any other value is
   rejected with `AACENC_INVALID_CONFIG`.
2. The value is stored in `USER_PARAM.userBitrateMode`
   (`aacenc_lib.cpp:164-201`, `:1296-1305`) and does not reconfigure the
   encoder immediately; it raises `AACENC_INIT_CONFIG`.
3. On the next init, `FDKaacEnc_AdjustEncSettings`
   (`aacenc_lib.cpp:544-682`) enforces the stripped plane:
   - AOT must be `AOT_AAC_LC` (`:557-560`);
   - mode must be `AACENC_BR_MODE_VBR_5` (`:561-563`);
   - afterburner is set to 1 (`:564-565`);
   - the bitrate is replaced by
     `FDKaacEnc_GetVBRBitrate(AACENC_BR_MODE_VBR_5, channelMode)`
     (`:619-621`);
   - PNS is cleared (`:665-666`).
4. `FDKaacEnc_Initialize` (`aacenc.cpp:428-760`) performs the actual
   configuration:
   - sample-rate whitelist (`:452-468`);
   - `FDKaacEnc_LimitBitrate` sanity clamp (`:475-485`);
   - `FDKaacEnc_DetermineBandWidth` (`:549-553`);
   - `FDKaacEnc_psyInit` / `FDKaacEnc_psyMainInit` (`:562-573`);
   - VBR QC initialization branch (`:582-591`);
   - mode mapping to `QCDATA_BR_MODE_VBR_5` (`:686-704`);
   - `qcInit.invQuant = 2` (`:716`);
   - `qcInit.maxIterations = 5` for non-low-delay (`:726-731`);
   - `qcInit.bitrate` and static transport bits (`:736-741`).
5. `FDKaacEnc_QCInit` (`qc_main.cpp:358-426`) copies the QC plane, selects the
   VBR-5 quality factor (`:389-397`), sets the LC dead-zone quantizer off
   (`:399-411`), and calls `FDKaacEnc_AdjThrInit` (`:413-422`).

`FDKaacEnc_AdjustVBRBitrateMode` exists (`aacenc.cpp:258-291`) but has no
caller in the stripped tree. A user-supplied bitrate therefore cannot
downgrade or upgrade the mode; it is accepted and ignored
(`aacenc_lib.cpp:1289-1295`).

### 3.2 Effective values at the validated operating points

The following are the values produced by the chain above. The QC values are
also captured verbatim in committed C-oracle fixtures
(`rust/src/aac_enc/testdata/qc6init_*.txt`).

| Quantity | 48 kHz mono | 48 kHz stereo | 44.1 kHz stereo |
|---|---:|---:|---:|
| Per-channel nominal bitrate | 112,000 bit/s | 96,000 bit/s | 96,000 bit/s |
| Total nominal bitrate | 112,000 bit/s | 192,000 bit/s | 192,000 bit/s |
| `averageBits` after byte alignment | 2,392 bits/frame | 4,096 bits/frame | 4,464 bits/frame |
| `maxBits` = `bitRes` | 6,144 bits | 12,288 bits | 12,288 bits |
| `minBits` | 0 | 0 | 0 |
| `bitResMode` | FULL | FULL | FULL |
| `meanPe` | 8,231 | 8,231 | 8,959 |
| `peMin` / `peMax` | 3,292 / 4,939 | 3,292 / 4,939 | 3,583 / 5,375 |
| `bits2PeFactor` | 1.18 (`m=0x4B851E80`, `e=1`) | 1.18 | 1.18 |

Sources: `aacenc.cpp:199-205,582-605,675-684`; `adj_thr.cpp:318-427,2463-2598`;
C oracle dumps `qc6init_mono-48k.txt`, `qc6init_stereo-48k.txt`,
`qc6init_stereo-44100.txt` (mapped in `rust/src/aac_enc/testdata.rs:2049-2188`).

For 44.1 kHz mono the formula gives 2,600 bits/frame after alignment and the
same `meanPe` as 44.1 kHz stereo (8,959), but no committed `qc6init` fixture
exists for that combination; those two values are marked **derived, not
fixture-captured**.

### 3.3 Per-frame path

For each frame, `FDKaacEnc_EncodeFrame` (`aacenc.cpp:769`) runs:

1. block switching per channel and stereo synchronization;
2. forward MDCT with the selected window sequence;
3. psychoacoustic analysis: energies, spreading, thresholds, pre-echo, TNS,
   M/S energies;
4. VBR threshold adaptation (`FDKaacEnc_AdaptThresholdsVBR`,
   `adj_thr.cpp:2160-2194`);
5. scalefactor estimation with `invQuant = 2`
   (`FDKaacEnc_EstimateScaleFactors`, `qc_main.cpp:870-873`);
6. quantize, count bits, section, and run the gain loop
   (`qc_main.cpp:788-1107`);
7. fill/finalize/reservoir update (`qc_main.cpp:1168-1274`);
8. element write and ADTS framing (`bitenc.cpp:1182-1322`;
   `tpenc_adts.cpp:192-237`).

The VBR-5 values affect step 3 through the bandwidth/active-band limit and
step 4 through the quality factor; steps 1, 2, 5 and 6 are common AAC-LC
machinery, and step 7 uses the VBR accounting branches.

---

## 4. Explicit VBR-5 settings

### 4.1 Settings selected by the VBR-5 tables and branches

The table uses the requested columns: setting, effective C value, meaning,
Rust equivalent, evidence, status. Status uses the classification from §7:
`1` match, `2` match intent with implementation difference, `3` deviation,
`4` not applicable/not implemented. "Match" here means the Rust code carries
the same effective value or branch, not that the whole encoder is equivalent.

| Setting | Effective C value | Meaning | Rust equivalent | Evidence | Status |
|---|---|---|---|---|---|
| Bitrate mode | `AACENC_BR_MODE_VBR_5 = 5`; `QCDATA_BR_MODE_VBR_5 = 5` | Selects the VBR-5 row and the shared VBR branches | `BITRATE_MODE_VBR_5 = 5`; the only accepted mode | `aacenc.h:195`; `qc_data.h:121`; `config.rs:109` | 1 |
| Per-channel rate, mono | 112,000 bit/s | Table entry; total 112 kbit/s for one effective channel | `VBR_5_CHAN_BITRATE_MONO` | `aacenc.cpp:205`; `config.rs:115` | 1 |
| Per-channel rate, stereo and multichannel | 96,000 bit/s | Table entry; total 192 kbit/s for two effective channels, 480 kbit/s for 5.1 | `VBR_5_CHAN_BITRATE_STEREO`; `total_br = 192000` (formula supports more channels; production is 1–2) | `aacenc.cpp:205,216-246`; `config.rs:117`; `stream_driver.rs:563-567` | 1 |
| Effective bandwidth | 19,293 Hz | Table value used for the low-pass line and active SFB count; 823 lines at 48 kHz, 895 lines at 44.1 kHz | `VBR_5_BANDWIDTH_HZ = 19293`; passed to `PsyTables::for_rate` | `bandwidth.cpp:158,273-295`; `config.rs:113`; `stream_driver.rs:569-576` | 1 (44.1/48 kHz) |
| VBR quality factor | `FL2FXCONST_DBL(0.070f)` = `0x08F5C290` | Scale of the per-band threshold lift; a smaller value lifts thresholds less and therefore spends more bits to meet them | `VBR_5_QUAL_FACTOR = 0.070`; `VBR_QUAL_VBR5` Q31 literal | `qc_main.cpp:131-132`; `adj_thr.cpp:2552`; `threshold_vbr.rs:140`; `config.rs:111` | 1 |
| Average frame bits | `(averageBitsPerFrame + 7) & ~7`; 4,096 (48 k stereo), 4,464 (44.1 k stereo), 2,392 (48 k mono) | Nominal budget used for PE/`meanPe`/`maxBitFac` and element-bit initialization; **not** the grant in the VBR path | `derive_qc_init` `average_bits` | `aacenc.cpp:583`; `rate.rs:410-424`; `qc6init_*.txt` | 1 |
| Maximum frame bits / reservoir size | `6144 × nChannelsEff`; 12,288 stereo, 6,144 mono | Hard per-frame cap and nominal reservoir size, in bits | `MIN_BUFSIZE_PER_EFF_CHAN = 6144`; `max_bits`, `bit_res` | `aacenc.h:111`; `aacenc.cpp:584-585`; `rate.rs:223,420-424` | 1 |
| Minimum frame bits | 0 | No minimum; fill only aligns | `min_bits = 0` | `aacenc.cpp:589-591`; `rate.rs:425` | 1 |
| Bit-reservoir mode | FULL | Reservoir pinned; no cross-frame carry; VBR threshold path selected | `BitResMode::Full` | `aacenc.cpp:647-660`; `rate.rs:426-432` | 1 |
| Bit-reservoir transport signal | `FDK_INT_MAX` | Tells the transport "variable rate"; ADTS writes `buffer_fullness = 0x7FF` | `ADTS_VBR_BUFFER_FULLNESS = 0x7FF` | `aacenc.cpp:295-310`; `adts.rs:119` | 1 |

### 4.2 Settings derived from the VBR-5 values

These are not independent table entries, but they change with VBR-5 and are
needed to understand the profile:

| Derived setting | Effective C value | Derivation | Rust equivalent | Status |
|---|---|---|---|---|
| Low-pass line | 823 (48 kHz long), 895 (44.1 kHz long), 102 (48 kHz short), 111 (44.1 kHz short) | `floor(2 × bandwidth × frameLength / sampleRate)` | `SfbConfig::lowpass_line` | 1 |
| Active SFB count | 45 long / 13 short at 48 kHz; 47 long / 13 short at 44.1 kHz | First SFB whose offset reaches the low-pass line | `sfb_active` | 1 |
| `allowIS` | Stereo: true (96,000 / 19,293 = 4, integer division, `< 5`); mono: false (112,000 / 19,293 = 5) | `useIS && (bitrate / bandwidth) < 5` | `allow_is` computed; IS itself not implemented | 3 (see §7.2) |
| `allowMS` | true | `useMS = 1` | `allow_ms = true` | 1 configuration |
| Long-window spreading high factor | 2.0 | `bitrate > 20,000` selects `MASKHIGHSPRENLONG` | Q31 table value | 1 |
| TNS stop band | 40 (48 kHz long), 42 (44.1 kHz long) | `min(tnsMaxBands, sfbActive)` | `TnsConfig::stop_band` | 1 |
| `meanPe` | 8,231 (48 kHz), 8,959 (44.1 kHz) | `10 × frameLength × bandwidth / (sampleRate / 2)` through fixed-point | `derive_qc_init` | 1 |
| Long TNS order / coefRes | 12 / 4 | Common LC TNS config, bitrate ≥ 16 kbit/s | `TnsConfig::max_order`, `coef_res` | 1 |
| Short TNS order / coefRes | 5 / 3 | Common LC TNS config | same | 1 |

The `allowIS` result is a direct consequence of the VBR-5 bitrate and
bandwidth values, not a separate VBR-5 switch. It matters because the C
encoder is *permitted* to use intensity stereo on stereo VBR-5 content, while
the Rust encoder does not implement the IS decision (§8, D2).

---

## 5. Inherited defaults and independent options

### 5.1 Inherited AAC-LC configuration

`FDKaacEnc_AacInitDefaultConfig` (`aacenc.cpp:337-368`) and
`aacEncDefaultConfig` (`aacenc_lib.cpp:445-488`) provide:

| Setting | Effective C value | Meaning | Rust equivalent | Status |
|---|---|---|---|---|
| Audio object type | AAC-LC (`AOT_AAC_LC = 2`) | Only accepted AOT | `AOT_AAC_LC = 2`; `Encoder::new` accepts only LC | 1 |
| Frame length | 1024 samples | Long frame; 960 is accepted by `AdjustEncSettings` but rejected by `Initialize` for non-LD | `FRAME_LENGTH = 1024`; 960 rejected | 1 |
| Transport | ADTS default, RAW allowed; LATM/LOAS/ADIF rejected | Seven-byte ADTS header, no CRC, one raw block | `TransportType::Adts`/`Raw` (`config.rs:132-139`); production API ADTS only | 1 for ADTS, 4 for RAW exposure |
| TNS | Enabled, mask `0xF` (long and short) | Common AAC-LC tool, signal-controlled | `TnsConfig.active = true` for long/short | 1 configuration, 2 implementation |
| M/S | Enabled (`useMS = 1`) | Common AAC-LC stereo tool | `allow_ms = true` | 1 configuration, 2 implementation |
| Intensity stereo | Enabled (`useIS = 1`) | Common AAC-LC stereo tool; `allowIS` decides per configuration | `allow_is` computed, decision not implemented | 3 |
| PNS | Forced off in this tree (`usePns = 0`) | No perceptual noise substitution | No PNS path; `NO_NOISE_PNS` | 1 configuration, 4 implementation |
| Afterburner | Forced on (`invQuant = 2`) | Analysis-by-synthesis scalefactor search | `estimate_scale_factors_channel*` always `inv = 2` | 1 |
| User bitrate | Ignored | The table value wins | No bitrate parameter | 1 |
| Channel order | MPEG order by default; demo uses WAV order (`CHANNELORDER=1`) | Multichannel input mapping | Mono/stereo only; no order issue | 1 for mono/stereo |
| Ancillary data | 0 | No ancillary bitrate | None | 1 |
| Metadata | Disabled by the strip | No metadata payload | None | 1 |

### 5.2 Afterburner

Upstream FDK exposes `AACENC_AFTERBURNER` with default 0
(`aacenc_lib.h:1351-1362`). The stripped reference accepts only the value 1
and reports 1 (`aacenc_lib.cpp:1367-1372,1543-1545`). The effective mechanism
is `qcInit.invQuant = 2` (`aacenc.cpp:716`), which selects the full
`EstimateScaleFactorsChannel` path:

- initial scalefactor formula;
- `improveScf` analysis-by-synthesis using quantize → inverse-quantize →
  distortion measurement (`sf_estim.cpp:378-452,1153-1164`);
- `assimilateSingleScf` (`:1170-1188`);
- `assimilateMultipleScf` and `assimilateMultipleScf2`
  (`:1189-1199`, gated by `invQuant > 1`).

It does not change the VBR quality factor, the thresholds, the bandwidth, or
the frame cap. It changes how well the chosen scalefactors fit the thresholds,
and therefore can change both distortion and bit consumption. In this
reference it is always on; the Rust production entry points always use
`inv = 2`, while a test-only entry covers the `0/1` checkpoints
(`scf.rs:1106-1145`). An upstream FDK build with afterburner off is therefore
a *different effective configuration* even at VBR mode 5, and is not the
reference audited here.

### 5.3 PNS and intensity stereo in this reference

- **PNS [Source]**: `hAacConfig->usePns = 0` (`aacenc_lib.cpp:665-666`);
  `FDKaacEnc_PnsDetect` returns immediately when `usePns == 0`
  (`aacenc_pns.cpp:192-194`); `CodePnsChannel` writes `NO_NOISE_PNS`.
  Rust passes `NO_NOISE_PNS` and zero books (`stream_driver.rs:34-36,1172-1175`).
- **IS [Source]**: `allowIS` is true for stereo because 96,000/19,293 = 4
  in integer division (`psy_configuration.cpp:565`). IS also requires a common
  window (`psy_main.cpp:1187,1213`). The C encoder can therefore set
  `isBook`/`isScale` on suitable stereo content. Rust computes `allow_is` but
  passes zero books (`stream_driver.rs:999-1025`) and does not implement the
  decision. This is discussed as D2 in §8.

### 5.4 Limits

- **Sample rates [Source]**: the C encoder accepts 8,000; 11,025; 12,000;
  16,000; 22,050; 24,000; 32,000; 44,100; 48,000; 64,000; 88,200; 96,000 Hz
  (`aacenc.cpp:452-468`). The Rust `EncoderConfig` accepts the same list
  (`config.rs:125-128`), but the production `Encoder` restricts construction
  to 44,100/48,000 Hz, mono/stereo, ADTS (`encoder.rs:182-205`).
- **Channels [Source]**: C supports MPEG channel modes up to 8 channels
  (`aacenc.cpp:470-473`; `channel_map.cpp:139-155`). The Rust production path
  supports one SCE or one CPE only; 3–8 channels are rejected
  (`encoder.rs:184-194`; `frame.rs:150-157`).
- **Bitrate [Source]**: VBR ignores the user bitrate. `FDKaacEnc_LimitBitrate`
  enforces a minimum of 40 bits per channel per frame plus transport static
  bits, and a maximum of `6144 × nChannelsEff` bits per frame
  (`aacenc.cpp:148-193`). At 44.1/48 kHz the nominal 192/112 kbit/s values are
  inside those bounds and are unchanged.
- **Low-rate clamp [Source]**: the maximum is a frame-buffer bound, so at low
  sample rates it can bind before the nominal table value. Examples:
  8 kHz stereo caps 192,000 at 96,000 bit/s; 16 kHz stereo sits exactly at
  the 192,000 bit/s cap, where `averageBits == maxBits` and `bitResMode`
  becomes `DISABLED` (`aacenc.cpp:647-660`). At those points the mode is still
  VBR-5, but `isCBRAdjustment` becomes 1, so `QCMain` uses the average budget
  and `AdjustThresholds` takes the CBR/PE path
  (`qc_main.cpp:798-800,2810`). These rates are outside the Rust validated
  matrix.
- **ADTS output [Source]**: 56-bit header, no CRC, one raw data block, 13-bit
  frame-length field, `buffer_fullness = 0x7FF`. The encoder cap implies a
  maximum ADTS frame of 1,543 bytes for stereo (12,288 payload bits + 56
  header bits) and 775 bytes for mono (6,144 + 56), well below the 8,191-byte
  ADTS field limit (`adts.rs:115-119,167-190`).
- **Frame length [Source]**: 960 is accepted by `AdjustEncSettings`
  (`aacenc_lib.cpp:608-613`) but `FDKaacEnc_Initialize` rejects it for
  non-low-delay AOTs (`aacenc.cpp:496-514`), so only 1024 is constructible.
  Rust rejects 960 (`config.rs:191-199`).

---

## 6. Runtime encoding behaviour

### 6.1 Frame pipeline and tool interaction

The pipeline is the standard FDK AAC-LC chain, with VBR acting on thresholds
and frame accounting rather than on the transform or stereo tools:

1. **Block switching [Source]**: eight 128-sample analysis windows, a
   second-order high-pass, a running energy average (`0.7` history + `0.3`
   current), an attack when `0.1 × windowEnergy > average`, and a minimum
   energy gate (`block_switch.cpp:122-145,168-189,290-345`). LC enables short
   frames and look-ahead (`:177-179`). The previous sequence and previous
   attack index are part of the decision, so the same frame can be classified
   differently depending on history.
2. **MDCT [Source]**: 1024 lines for long/start/stop, 8 × 128 for short;
   KBD for long, SINE for start/short, KBD for stop
   (`block_switch.cpp:122-124`; `transform.cpp:117-167`).
3. **Psychoacoustics [Source]**: band energies, spreading, threshold
   initialization, PCM floor, pre-echo control, TNS and M/S energies
   (`psy_main.cpp:552-1130`). The VBR-5 bandwidth limits the active bands to
   45 long at 48 kHz, 47 long at 44.1 kHz, and 13 short at both rates.
4. **VBR threshold adaptation [Source]**: `AdaptThresholdsVBR`
   (`adj_thr.cpp:2160-2194`) calls `adaptMinSnr`, `initAvoidHoleFlag`, and
   `reduceThresholdsVBR` (`:1108-1312`). The threshold lift is
   `redVal = vbrQualFactor × chaosMeasure × frameEnergy^(1/4)`, with
   short-block group corrections; a larger factor lifts thresholds more and
   spends fewer bits. VBR-5's 0.070 is the smallest factor in the table
   (`qc_main.cpp:122-132`), so it is the least aggressive of the VBR modes in
   this mechanism. The table is not monotonic across mode numbers: VBR-3 uses
   0.176 and VBR-4 uses 0.120, so mode number alone does not order quality.
   [Interpretation: this is a monotonic statement about the formula, not a
   listening result.]
5. **Scalefactors and quantization [Source]**: `EstimateScaleFactors` with
   `invQuant = 2` performs the afterburner search (`sf_estim.cpp:1046-1292`);
   `quantize.cpp:116-169,278-301` applies the AAC quantizer with
   `MAX_QUANT = 8191` and the LC dead zone (`dZoneQuantEnable = 0`,
   `qc_main.cpp:399-411`).
6. **Bit counting and sectioning [Source]**: `dynBitCount` chooses codebooks
   and sections from the quantized spectrum (`dyn_bits.cpp`); this is the
   actual bit cost, not an estimate.
7. **Gain loop [Source]**: if the counted bits exceed the element grant or the
   `6144`-bit/channel cap, `reduceBitConsumption` bumps gains and requantizes,
   up to `maxIterations = 5`; if that fails, crash recovery drops bands
   (`qc_main.cpp:901-1107,1395-1498`).
8. **Fill/finalize [Source]**: VBR fill adds only byte alignment and the
   minimum-frame pad (`minBits = 0`), and the reservoir is pinned
   (`qc_main.cpp:1168-1209,1249-1274`).

### 6.2 Bit allocation without a bit target

This is the central mechanism of VBR-5.

- `FDKaacEnc_QCMain` computes `isCBRAdjustment` as false when the mode is VBR
  and `bitResMode == FULL` (`qc_main.cpp:798-800`). At the validated 44.1/48
  rates it is false.
- With `isCBRAdjustment == 0`, both `BitResRedistribution` and
  `prepareBitDistribution` are called with `maxBitsPerFrame`, not with the
  nominal average (`qc_main.cpp:805-806,836-839`). The grant is therefore
  derived from the 6,144-bit/effective-channel cap, not from 192/112 kbit/s:
  `grantedDynBits = ((maxBits - 56) & ~7) - (3 + staticBits)`.
- `FDKaacEnc_AdjustThresholds` takes the non-CBR branch
  (`adj_thr.cpp:2810,2869-2884`), which calls only `AdaptThresholdsVBR`. The
  granted PE is not used to lower the thresholds. In other words, the encoder
  does not allocate a fixed number of bits to a frame and then fit the signal
  into it; it derives thresholds from the signal and the fixed quality factor,
  and uses the bit cap only as a safety limit.
- `FDKaacEnc_DistributeBits` still computes PE/bit factors
  (`adj_thr.cpp:2723-2798`), but in the VBR threshold path those values do not
  drive the threshold reduction. They update the ATS state and remain
  available for the CBR branches; they are not a per-band bit budget for the
  VBR threshold decision.
- Because the reservoir is pinned full but `maxBits - avgTotalBits` is zero in
  the VBR call, `BitResRedistribution` yields level 0 (`qc_main.cpp:750-753`).
  No bits are carried between frames. Each frame's bit budget is
  self-contained under the cap.

**Not fixed by the preset:** the window sequence, shape and grouping; the TNS
active flags, order and coefficients; the M/S digest and per-band mask; the IS
books/scales in C; scalefactors and global gain; whether the gain loop needs a
second pass or crash recovery; the fill bits; and the resulting frame size and
average rate. The preset fixes the bandwidth, quality factor, nominal rate
table and accounting rules; these decisions remain per-frame and
signal-dependent.

**Consequence [Interpretation]:** "VBR-5" is a quality-targeted mode with a
generous ceiling. The nominal 192 kbit/s (stereo) is an initialization value
for PE and element-bit state; it is not a target, a minimum, or a guaranteed
average. This is why the same mode can produce very different average rates on
different content (§6.3).

### 6.3 Why bitrate varies between tracks and across frames

The frame size is the sum of static side information, Huffman-coded spectrum
and scalefactors, plus 0–7 alignment bits. It varies because:

- the threshold lift depends on the frame's chaos measure and energy
  (`adj_thr.cpp:1108-1231`);
- tonal frames and noise-like frames produce different SFB energies,
  spreading, and quantizer demands;
- long versus short windows change the number of coded bands, grouping and
  TNS configuration;
- stereo M/S changes which bands are coded as sum/difference;
- TNS can reshape the spectrum and change its coding cost;
- the gain loop may requantize or, in the limit, drop bands;
- there is no reservoir smoothing and no average-frame target to pull a quiet
  frame up or a loud frame down.

Measured examples (these are encoder outputs, not quality scores):

| Material | C output | Rust output | Duration / basis | Evidence |
|---|---:|---:|---|---|
| 48 kHz stereo orchestral excerpt (M8/Eroica) | 235,444 bytes ≈ 188.4 kbit/s | 234,229 bytes ≈ 187.4 kbit/s | 10.000 s, 480,000 input frames, ADTS bytes | C size: `aac-enc-m8-owner-decision.md:46`; Rust size: `aac-enc-m5g-branch-execution-census.md:51`; duration: `aac-enc-m8-owner-decision.md:34-40` |
| 48 kHz mono click | 2,739 bytes ≈ 43.8 kbit/s | 2,680 bytes ≈ 42.9 kbit/s | 0.5 s synthetic fixture | `aac-enc-m5b-objective-quality-baseline.md:23-34` |
| 48 kHz stereo onset | 2,318 bytes ≈ 18.5 kbit/s | 2,359 bytes ≈ 18.9 kbit/s | 1.0 s synthetic fixture | `aac-enc-m5b-objective-quality-baseline.md:36-47` |
| 44.1 kHz stereo sine | 3,014 bytes ≈ 24.1 kbit/s | 2,998 bytes ≈ 24.0 kbit/s | 1.0 s synthetic fixture | `aac-enc-m5b-objective-quality-baseline.md:49-62` |

The synthetic figures are far below the 192 kbit/s nominal because the
fixtures are short and sparse; the orchestral excerpt lands close to the
nominal rate. This spread is the expected consequence of a threshold-targeted
mode, not evidence that one measurement is "correct" and the other is not.
The M8 fixture is `PROVISIONAL` (`aac-enc-m8-owner-decision.md:1-20`), so its
bitrate is illustrative and is not a release-approved corpus statistic.

Across frames within one file, the same mechanism applies. For example, the
M5.b baseline records `impulse-mono` frames ranging from 13-byte floor frames
to a 178-byte attack frame, with the same encoder configuration
(`aac-enc-m5b-objective-quality-baseline.md`;
`aac-enc-migration-spec.md` §6 baseline). The cap is a safety bound; the
cited average frame sizes are well below it.

### 6.4 What the profile does not guarantee

- **No fixed bitrate.** The nominal table is not a target; measured content
  ranges from ~18 kbit/s on sparse synthetic material to ~188 kbit/s on
  orchestral material in the cited corpus.
- **No guaranteed transparency.** The quality factor is a heuristic threshold
  scale. No listening test, ABX test, or perceptual metric is part of the
  evidence in this repository.
- **No guarantee of a particular frame size or frame-size distribution.**
  Only the maximum is bounded by the 6,144-bit/effective-channel cap.
- **No guarantee of C bit-exactness.** Structural equality and some
  byte-identical frames are established; full payload equality is not
  (see §7.3 and §8).
- **No guarantee that 228 kbit/s is reproduced.** The public header's
  "approx. 228 kbps for stereo" (`aacenc_lib.h:451-458`) is an approximate
  average over content and settings. The source does not record how it was
  measured; the derivation of that figure is `Not established`, and it is not
  the internal 192 kbit/s table value. It is recorded here as documentation,
  not as an acceptance criterion.
- **No guarantee for configurations outside the validated matrix** (other
  sample rates, more than two channels, RAW output through the production API).

### 6.5 How psychoacoustics, quantization, block switching and stereo interact

[Interpretation, grounded in the sources above.]

The block switch decides the time-frequency tiling. A transient selects short
windows, which change the SFB geometry, the TNS order/coefficient resolution,
and the short-block threshold correction. The psychoacoustic stage then
computes band energies and masking thresholds for that tiling. VBR raises those
thresholds by an amount proportional to the frame's chaos measure and to the
fourth root of the frame energy, scaled by 0.070. The scalefactor search (with
afterburner) chooses per-band scalefactors that meet the raised thresholds
using real quantize / inverse-quantize / distortion measurements. M/S can
replace L/R by sum/difference in bands where that lowers the estimated noise;
on `MS_ALL` it extends the mask to the remaining non-IS bands. IS, when
enabled in C, can replace the right channel's spectrum in highly correlated
bands. Finally, the Huffman sectioning and gain loop turn the quantized
spectrum into an actual bit count and enforce the cap.

The result is a feedback system with no single "bitrate knob": changing the
signal changes the thresholds, which change the scalefactors, which change the
bits. The preset fixes the target *policy* (bandwidth, quality factor, rate
tables); it does not fix the outcome.

---

## 7. C-to-Rust implementation comparison

### 7.1 Rust architecture

The production path is:

`Encoder` (`encoder.rs:166`) → `PcmFeeder` (`pcm.rs`) →
`StreamEncoder::step_frame` (`stream_driver.rs:854`) → per-frame modules:

| Stage | Rust module | Nature |
|---|---|---|
| Block switching | `block_switch.rs` | f32-native port of the LC block switch |
| Forward MDCT | `transform.rs` + `common::mdct` | float port (f64 spectrum, f32 block-switch analysis); absolute scale pinned |
| Band energy / thresholds | `energy.rs`, `thresholds.rs` | integer mirror with a float-to-Q31 spectrum input |
| Psychoacoustic orchestration | `psy.rs` | float spectrum + integer threshold chain |
| TNS | `tns.rs` | exact integer autocorrelation; float Schur |
| VBR threshold adaptation | `threshold_vbr.rs` | Q31 integer mirror |
| Scalefactor search / afterburner | `scf.rs` | exact integer mirror, `invQuant = 2` |
| Quantizer | `quant.rs` | exact integer mirror |
| Bit counting / sections | `bitcount.rs` | exact integer mirror |
| Rate control / gain loop | `rate.rs` | exact integer mirror of VBR branches |
| Frame assembly / ADTS | `frame.rs`, `adts.rs` | exact bit writer and header writer |
| Stereo M/S | `stream_driver.rs::ms_stereo_mirror` | exact integer mirror, float-derived inputs |
| Intensity stereo | — | not implemented |
| PNS | — | not implemented; configuration is off |

The Rust module docs themselves state the intended fidelity. For example,
`quant.rs:94-140` and `scf.rs:94-189` describe exact integer mirrors;
`energy.rs:107-112` calls the float-to-int spectrum mapping the single
approximation point; `thresholds.rs:94-101` says every later integer op is
bit-exact given identical inputs; `tns.rs:94-116` documents both the exact
autocorrelation lane and the float Schur lane.

### 7.2 Comparison by area

| Area | C reference | Rust implementation | Classification | Evidence |
|---|---|---|---|---|
| Profile selection and configuration | AAC-LC, VBR-5, afterburner on, PNS off, ADTS/RAW, 1024 frames | Same enforced constants; production constructor narrows to 44.1/48 kHz, mono/stereo, ADTS | **1** at validated scope; scope narrowing is a documented limitation | `aacenc_lib.cpp:544-682`; `config.rs:104-205`; `encoder.rs:182-205` |
| Init-time psy tables (SFB offsets, spreading, PCM floor, minSnr) | Computed by `FDKaacEnc_InitPsyConfiguration` from rate, bandwidth, bitrate | SFB/spreading/PCM tables are committed Q31 tables; `minSnr` comes from committed C snapshots for 44.1/48 kHz × 96/112 kbit/s | **1** at the four validated points (exact by construction); **4** as a general derivation (Rust does not recompute minSnr from the formula) | `psy_configuration.cpp:534-627`; `sfb.rs:232-355`; `psy_config.rs:29-55`; `stream_driver.rs:556-577` |
| Masking thresholds and spreading | Integer threshold chain on C's stored energies | Integer chain on a float-derived Q31 spectrum; all later ops exact | **2** — same intent, float front-end residue | `thresholds.rs:94-120`; `energy.rs:107-112`; `psy.rs:103-113` |
| Block switching / short windows | Q31 8-window attack detector, look-ahead, KBD/SINE shapes | f32 equivalent; window sequences verified equal on tested fixture prefixes | **2** — same decisions on tested content, not a bit-exact energy mirror | `block_switch.cpp:122-145,168-345`; `block_switch.rs:94-135`; M5.25 D-020 |
| Forward MDCT | Fixed-point MDCT | Float MDCT with pinned absolute scale | **2** — implementation difference; output residue feeds later decisions | `transform.rs:94-127` |
| TNS | Integer autocorrelation + fixed-point Schur; max order 12/5, coefRes 4/3 | Exact integer autocorrelation; float Schur; same config | **2** for mechanism, **3** for 40 pinned decision flips (active-filter cases have unverified quality impact) | `aacenc_tns.cpp:377-503`; `tns.rs:94-116,185-246`; M5.25 D-012/D-013 |
| Stereo M/S | Threshold-based per-band decision; `MS_NONE/SOME/ALL`; allows all non-IS bands to become M/S | Exact integer mirror of `MsStereoProcessing`; `allow_ms = true` | **2** — exact logic, float-derived operands; one known band-16 flip | `ms_stereo.cpp:109-295`; `stream_driver.rs:235-389,964-987`; M5.26 |
| Intensity stereo | Enabled by default; `allowIS = true` for stereo VBR-5; requires common window and correlation/region tests | Not implemented; zero books are passed | **3** — C can use IS on suitable stereo content; Rust cannot | `intensity.cpp:614-639`; `psy_configuration.cpp:565`; `stream_driver.rs:34-36,999-1025` |
| PNS | Forced off in this tree | Not implemented; zero noise books | **4** — not applicable to this profile; the C path is disabled by configuration and Rust has no path | `aacenc_lib.cpp:665-666`; `aacenc_pns.cpp:192-194`; `stream_driver.rs:1172-1175` |
| Quantization | Fixed-point quantizer, dead zone off, `MAX_QUANT = 8191` | Exact integer mirror | **1** | `quantize.cpp:116-301`; `quant.rs:94-140` |
| Scalefactor search / afterburner | `invQuant = 2` full search | Exact integer mirror, `inv = 2` only | **1** (with the `q == -32768` guard difference below) | `sf_estim.cpp:1046-1292`; `scf.rs:94-189,1106-1145` |
| Bit counting / sectioning | `dynBitCount` + Huffman kernels | Exact integer mirror | **1** | `dyn_bits.cpp`, `bit_cnt.cpp`; `bitcount.rs:94-190` |
| Rate control / gain loop | VBR branches: grant from maxBits, no reservoir carry, fill alignment, up to 5 iterations, crash recovery | Exact integer mirror; VBR branches only; single-pass policy in the driver | **1** for single-pass; **3** for multi-pass/crash frames (Rust refuses to emit them) | `qc_main.cpp:788-1107,1168-1274`; `rate.rs:100-230,1420-1768`; `stream_driver.rs:1297-1310` |
| Frame assembly / ADTS | SCE/CPE/LFE, fill, `ID_END`, 56-bit ADTS, 13-bit length, `0x7FF` fullness | SCE/CPE only; exact bit writer; same ADTS header | **1** for mono/stereo; **4** for LFE/multi-element | `bitenc.cpp:1182-1322`; `tpenc_adts.cpp:192-237`; `frame.rs:94-185,825-1141`; `adts.rs:94-190` |
| Flush / delay | Zero-pad to `nDelay = 1600`; output ends after the padded frames | Same law; `PcmFeeder::finish` | **1** | `aacenc_lib.cpp:137,1092-1109`; `pcm.rs:261-283`; `config.rs:272-278` |
| RAW transport | Supported in C (`TT_MP4_RAW`) | `EncoderConfig` models `TransportType::Raw`, but `Encoder::new` requires ADTS | **4** for production API; configuration model exists | `aacenc_lib.cpp:593-607`; `config.rs:130-139`; `encoder.rs:184-194` |
| Bandwidth / peak-bitrate overrides | `AACENC_BANDWIDTH`, `AACENC_PEAK_BITRATE` can change the effective plane | Not exposed; fixed 19,293 Hz and default cap | **4** — defaults match, overrides are outside the Rust API | `bandwidth.cpp:273-279`; `aacenc_lib.cpp:626-640,1349-1354,1487-1492` |
| 960-sample granule | Accepted by `AdjustEncSettings`, rejected for LC by `Initialize` | Rejected | **1** — effective C behaviour is also rejection | `aacenc_lib.cpp:608-613`; `aacenc.cpp:496-514`; `config.rs:191-199` |

One benign representation difference is worth recording. `SfbConfig::new`
clamps the bandwidth argument to `sample_rate / 2` (`sfb.rs:246-248`), while
the C VBR branch stores 19,293 Hz unchanged (`bandwidth.cpp:273-295`). At
44.1/48 kHz the clamp is inactive. Below roughly 38.6 kHz the two formulas
give different `lowpass_line` values (for example the raw C formula yields
1,234 at 32 kHz, the clamped value 1,024). The active-band count is unaffected
for the committed SFB tables because the table's last offset is already at the
transform length. The Rust all-rates config test compares against oracle
snapshots generated with the same clamp (`sfb.rs:614-655`;
`tools/enc-oracle/enc_oracle.cpp:312,365`), so it establishes parity with the
oracle configuration, not with a raw C `DetermineBandWidth` call at those
rates. This is outside the production scope and is not a profile change.

### 7.3 Profile differences versus implementation differences

These are distinct and should not be conflated:

**Profile settings fixed by this reference (two differ from upstream defaults):**

- Afterburner on. Upstream FDK's default is off; this tree forces it on.
  Rust matches the forced-on reference, not the upstream default.
- PNS off. Upstream AAC-LC's default is on; this tree forces it off. Rust
  matches the forced-off reference.
- User bitrate ignored; VBR-5 table value wins.
- AAC-LC only, 1024 frames, ADTS/RAW.

**Implementation differences (how the same profile is computed):**

- Float-native block switching, MDCT, Schur/TNS, and spectrum-to-Q31 mapping,
  with documented residue and pinned flips.
- No IS decision in Rust, although the C configuration allows IS for stereo.
- No multi-pass/crash frame emission in the Rust production API, although
  `rate.rs` implements the mechanism.
- Init-time `minSnr` comes from committed C snapshots for four operating
  points rather than being recomputed from the formula.
- Narrower transport/channel/rate scope in the public Rust `Encoder`.

A difference in implementation is not a different preset: it still matters
for equivalence, but it is audited as an implementation divergence rather than
a configuration mismatch. A difference in profile setting is more fundamental:
it changes what the reference encoder *is*, so the comparison target itself
must be restated.

### 7.4 What the existing evidence supports

- **Structural parity**: both encoders emit valid ADTS with the same core
  header fields. The 2026-10-21 audit reports, for 15 synthetic streams and
  558 frames, 558/558 frames with equal core ADTS fields, 286/558 complete
  frames byte-equal, and all differences confined to payload-dependent frame
  length (`aac-enc-release-gate-addendum-20261021.md` §4.2).
- **Stage parity where it is claimed**: S3–S8 (quantizer, scalefactor search,
  bit counting, rate control, element/bitstream writing) are exact integer
  mirrors. The oracle-fed differential in M5.26 found S4–S6 bit-exact given
  C's inputs (`aac-enc-first-divergence-m5.26.md` §1); S3, S7 and S8 are
  covered by their own exact fixtures (`quant.rs`, `bitenc.rs`, `frame.rs`).
- **No evidence of perceptual equivalence**: there is no listening test, ABX
  test, or perceptual metric in the repository. Structural and objective
  distance measurements do not establish transparency.

---

## 8. Known deviations and open questions

Severity is an engineering judgement about the acceptance target, not a
measured quality result.

| ID | Deviation / gap | Effect | Evidence | Severity |
|---|---|---|---|---|
| D1 | **Multi-pass / crash frames are not emitted by the Rust API.** `step_frame` returns no bytes when `passes.len() != 1`; crash frames are classified and discarded. C can encode them (possibly with reduced `maxSfb`) or fail through its own error paths. | A frame that needs requantization after the first pass becomes a terminal error in `Encoder`, not output. | `stream_driver.rs:1297-1310`; `encoder.rs:288-299,318-336`; M5.25 D-040; M5.i (0 occurrences in 2,442 corpus frames) | High for robustness; not observed in the corpus |
| D2 | **Intensity stereo is not implemented.** C computes `allowIS = true` for stereo VBR-5 and can set `isBook`/`isScale`; Rust passes zero books. | On strongly correlated, off-center stereo content C could use IS and Rust cannot; bitstream and rate may diverge. Corpus oracle dumps show no live IS, so this is unexercised, not disproven. | `psy_configuration.cpp:565`; `intensity.cpp:614-639`; `stream_driver.rs:34-36,999-1025`; M5.25 D-014 | Medium–high |
| D3 | **TNS decision flips on 40 pinned windows.** The documented likely cause is f32 MDCT residue crossing Q15 coefficient borders; active-filter cases change the encoded spectrum. | Bitstream/rate differences on transients; quality impact unverified. | `aac-enc-quality-divergence-audit-m5.25.md` D-012/D-013; `tns.rs:94-116` | Medium |
| D4 | **Float front-end residue** (MDCT, block-switch energies, spectrum-to-Q31 mapping) causes 21 `KNOWN_MAXSCALE` ±1 flips and at least one MS band-16 decision flip. | Mostly representational; decision flips are pinned and bounded. No general neutrality proof. | M5.25 D-011; M5.26 §1; `energy.rs:107-112` | Medium–low |
| D5 | **Init-time psy tables are snapshots, not derivations.** `minSnr` is embedded for 44.1/48 kHz × 96/112 kbit/s; the snapshot loader panics for other combinations. | Exact at the validated points; cannot generalize to other rates/bitrates without new snapshots. | `psy_config.rs:29-55`; `stream_driver.rs:568-576` | Medium for scope; none at validated points |
| D6 | **Rust production scope is narrower than the C profile**: 44.1/48 kHz, mono/stereo, ADTS only. | Other C-valid configurations are rejected at construction. | `encoder.rs:182-205` | Medium (documented limitation) |
| D7 | **No bandwidth or peak-bitrate override** in the Rust API. | Defaults match; expert C configurations cannot be reproduced. | `bandwidth.cpp:273-279`; `aacenc_lib.cpp:626-640` | Low |
| D8 | **`q == -32768` edge case**: C's `fixp_abs` guard wraps and later reads out of bounds; Rust panics loudly instead. | Out-of-envelope input only; committed fixtures exclude it. | `quant.rs:200-210`; M5.25 | Low |
| D9 | **`checkMinFrameBitsDemand` is a stub returning 1 in both C and Rust.** | Parity with a functional stub; two exit-matrix arms are dead by construction. | `qc_main.cpp:571-575`; `rate.rs:1131-1132`; M5.25 D-041 | Low (parity) |
| D10 | **No perceptual or bitrate-target validation.** | No claim of transparency or "correct" average bitrate is supported. | M5.25 §6.1 G1; §6.4 above | High for any quality claim |
| D11 | **M8/Eroica fixture is `PROVISIONAL`.** | The 187–188 kbit/s real-content comparison is illustrative, not release-approved. | `aac-enc-m8-owner-decision.md:1-20,46` | Evidence caveat |
| D12 | **Config layer accepts 12 rates, production constructor accepts 4 combinations.** | A caller can construct an `EncoderConfig` that `Encoder::new` later rejects. | `config.rs:125-205`; `encoder.rs:182-205` | Low (API clarity) |

### Open questions

1. **How often does C actually use IS at VBR-5 on real stereo content?**
   `allowIS` is true, but the correlation/region tests are strict. The corpus
   shows zero books; that is a corpus result, not a proof of non-occurrence.
2. **How often do multi-pass or crash frames occur on real content?** The
   measured 19-stream corpus shows zero in 2,442 frames, with at least 984 bits
   of headroom (M5.i §5). The mechanism is implemented but unexercised live.
3. **What is the perceptual effect of the 40 TNS decision flips?** The source
   shows the spectrum changes when the filter is active on both sides; no
   listening or objective quality metric exists.
4. **Can the public 228 kbit/s stereo figure be reproduced?** The source does
   not state the measurement conditions. It should not be used as an acceptance
   value.
5. **Should the Rust target expand to all C rates/channels?** That requires
   new psy snapshots, multi-element frame assembly, and LFE handling; it is a
   profile-scope decision, not a bug fix.

---

## 9. Evidence and source references

### 9.1 Principal C files

| File | Role in this profile |
|---|---|
| `cpp/libAACenc/include/aacenc_lib.h` | Public parameter values and documentation: VBR modes `:438-470`, bitrate mode `:1287-1305`, afterburner `:1351-1362` |
| `cpp/libAACenc/src/aacenc.h` | `MIN_BUFSIZE_PER_EFF_CHAN = 6144` `:111`; bitrate-mode enum `:189-201` |
| `cpp/libAACenc/src/aacenc.cpp` | VBR tables `:199-205`; `GetVBRBitrate` `:216-246`; transport reservoir `:295-310`; defaults `:337-368`; init `:428-760`; VBR QC init `:582-605`; `invQuant = 2` `:716`; `maxIterations` `:726-731` |
| `cpp/libAACenc/src/aacenc_lib.cpp` | Default config `:445-488`; enforced stripped settings `:544-682`; parameter application `:1278-1375`; getters `:1514-1545` |
| `cpp/libAACenc/src/bandwidth.cpp` | VBR bandwidth table `:152-162`; VBR selection `:260-310` |
| `cpp/libAACenc/src/qc_main.cpp` | VBR quality table `:122-133`; QC init `:358-426`; grant/distribution `:615-640`; QCMain `:788-806`; fill/bitres/finalize `:1168-1274,1283-1383` |
| `cpp/libAACenc/src/adj_thr.cpp` | VBR threshold reduction `:1108-1312`; `AdaptThresholdsVBR` `:2160-2194`; `AdjThrInit` `:2463-2598`; `DistributeBits` `:2723-2798`; `AdjustThresholds` `:2804-2900` |
| `cpp/libAACenc/src/psy_main.cpp` | Psy init `:302-390`; per-frame decisions `:552-1246` |
| `cpp/libAACenc/src/psy_configuration.cpp` | SFB/spreading/minSnr/allowIS/lowpass config `:534-627` |
| `cpp/libAACenc/src/aacenc_tns.cpp` | TNS config `:377-503` |
| `cpp/libAACenc/src/block_switch.cpp` | LC block switching `:122-145,168-345` |
| `cpp/libAACenc/src/ms_stereo.cpp` | M/S decision `:109-295` |
| `cpp/libAACenc/src/intensity.cpp` | IS decision and `allowIS` early-out `:614-639` |
| `cpp/libAACenc/src/quantize.cpp` | Quantizer/inverse quantizer `:116-301` |
| `cpp/libAACenc/src/sf_estim.cpp` | Scalefactor search / afterburner `:1046-1292` |
| `cpp/libAACenc/src/dyn_bits.cpp`, `bit_cnt.cpp` | Sectioning and noiseless bit counting |
| `cpp/libAACenc/src/bitenc.cpp` | Element/frame writing `:1182-1322` |
| `cpp/libMpegTPEnc/src/tpenc_adts.cpp` | ADTS header `:129-153,192-237` |

### 9.2 Principal Rust files

| File | Role |
|---|---|
| `rust/src/aac_enc/config.rs` | VBR-5 constants, whitelists, nominal rate, delay |
| `rust/src/aac_enc/encoder.rs` | Production `Encoder` API and fail-closed error mapping |
| `rust/src/aac_enc/stream_driver.rs` | Per-frame orchestration, M/S mirror, policy gates |
| `rust/src/aac_enc/sfb.rs` | SFB geometry, spreading tables, `allow_is`/`allow_ms` |
| `rust/src/aac_enc/psy_config.rs` | Embedded init-time C snapshots |
| `rust/src/aac_enc/psy.rs`, `thresholds.rs`, `energy.rs` | Psychoacoustic orchestration and threshold chain |
| `rust/src/aac_enc/block_switch.rs`, `transform.rs` | Block switching and MDCT |
| `rust/src/aac_enc/tns.rs` | TNS configuration and filtering |
| `rust/src/aac_enc/threshold_vbr.rs` | VBR threshold adaptation |
| `rust/src/aac_enc/scf.rs`, `quant.rs` | Scalefactor search/afterburner and quantizer |
| `rust/src/aac_enc/bitcount.rs`, `bitenc.rs` | Bit counting, sections, payload coding |
| `rust/src/aac_enc/rate.rs` | QC init, grants, gain loop, fill, reservoir |
| `rust/src/aac_enc/frame.rs`, `adts.rs` | Frame assembly and ADTS header |
| `rust/src/aac_enc/pcm.rs` | Input buffering and flush |

### 9.3 Measurement and fixture records

- [`aac-enc-m5b-objective-quality-baseline.md`](./aac-enc-m5b-objective-quality-baseline.md)
  — synthetic-fixture bitrates and objective baseline.
- [`aac-enc-m5i-pass-margin-census.md`](./aac-enc-m5i-pass-margin-census.md)
  — 2,442-frame pass-margin census (all single-pass, minimum headroom 984
  bits).
- [`aac-enc-m5g-branch-execution-census.md`](./aac-enc-m5g-branch-execution-census.md)
  — 19-stream ADTS byte counts and per-frame invariants.
- [`aac-enc-quality-divergence-audit-m5.25.md`](./aac-enc-quality-divergence-audit-m5.25.md)
  — divergence inventory (D-011, D-012, D-014, D-020, D-021, D-040).
- [`aac-enc-first-divergence-m5.26.md`](./aac-enc-first-divergence-m5.26.md)
  — stage attribution and the bit-exactness claim for oracle-fed S4–S8.
- [`aac-enc-release-gate-addendum-20261021.md`](./aac-enc-release-gate-addendum-20261021.md)
  — current ADTS audit (558-frame C/Rust header comparison).
- [`aac-enc-m8-owner-decision.md`](./aac-enc-m8-owner-decision.md) — provenance
  and C reference size for the Eroica fixture (`PROVISIONAL`).
- [`aac-enc-m6-api-contract.md`](./aac-enc-m6-api-contract.md) — production API
  scope and error contract.
- `rust/src/aac_enc/testdata/qc6init_*.txt` — C-captured QC init values used
  in §3.2.

### 9.4 What the tests do and do not prove

- They do prove: structural ADTS validity, exact stage arithmetic where
  claimed, corpus-wide sequence/grouping parity on tested prefixes, known
  divergence pinning, and no silent fallback on unsupported paths.
- They do not prove: perceptual equivalence, bit-exactness on arbitrary
  content, IS/multi-pass behaviour on live content, or a target bitrate.

---

## 10. Implications for validation and future changes

### 10.1 Invariants to preserve

Any change that claims to preserve the VBR-5 profile must preserve:

- `vbrQualFactor = 0.070` (`0x08F5C290`);
- per-channel nominal rates 112,000 / 96,000 bit/s;
- effective bandwidth 19,293 Hz at the validated rates;
- `maxBits = bitRes = 6144 × nChannelsEff` and `minBits = 0`;
- the VBR grant bases (`maxBitsPerFrame`, not the average);
- no cross-frame reservoir carry at VBR-5;
- VBR fill limited to byte alignment and the minimum-frame pad;
- afterburner `invQuant = 2` and PNS off, unless the profile definition is
  explicitly changed;
- ADTS structural fields (56-bit header, profile 1, one block, `0x7FF`
  fullness, 13-bit length).

### 10.2 Validation gates

A change should be checked at four levels:

1. **Structural**: ADTS parses, frame length matches bytes, no trailing or
   missing data.
2. **Stage-exact**: the existing S1–S8 oracle fixtures and the 20-stream
   migration equality test still pass.
3. **Corpus classification**: every C/Rust difference is classified as
   `exact`, `known`, or `new`; a new class fails the run. This is the existing
   `stream_driver` harness policy.
4. **Measurement**: report the actual frame-size distribution and average
   bitrate on fixed fixtures. Do not substitute the nominal 192/112 kbit/s for
   a measurement.

### 10.3 If the profile is changed

- Changing any constant in §10.1 requires re-running the corpus, updating this
  document, and re-baselining the committed hashes.
- Adding IS or multi-pass emission is not a profile change, but it changes the
  effective output on content where those tools fire. It requires new C-oracle
  fixtures and an explicit note that the previous corpus did not exercise the
  path.
- Do not describe a change as "quality improvement" without a listening or
  objective test that is actually run. The afterburner and VBR factor are
  mechanisms, not measured quality claims.

### 10.4 If the Rust scope expands

Support for more sample rates, more channels, or RAW output requires:

- new `minSnr`/spreading snapshots or a from-formula implementation with
  oracle validation;
- multi-element frame assembly and LFE handling;
- transport tests for RAW;
- a fresh C/Rust corpus at the new operating points.

Until then, the acceptance target is the validated four operating points
(44.1/48 kHz × mono/stereo), and the document should say so rather than
implying general FDK VBR-5 equivalence.

---

## Appendix A — Worked constants for the validated points

| Quantity | 44.1 kHz mono | 44.1 kHz stereo | 48 kHz mono | 48 kHz stereo |
|---|---:|---:|---:|---:|
| Nominal total bitrate | 112,000 | 192,000 | 112,000 | 192,000 |
| `averageBits` (aligned) | 2,600 (derived) | 4,464 (fixture) | 2,392 (fixture) | 4,096 (fixture) |
| `maxBits` / `bitRes` | 6,144 | 12,288 | 6,144 | 12,288 |
| `minBits` | 0 | 0 | 0 | 0 |
| `bitResMode` | FULL | FULL | FULL | FULL |
| `meanPe` | 8,959 (derived) | 8,959 (fixture) | 8,231 (fixture) | 8,231 (fixture) |
| Max ADTS frame (payload + header) | 775 B | 1,543 B | 775 B | 1,543 B |
| Low-pass line, long | 895 | 895 | 823 | 823 |
| Active SFB, long | 47 | 47 | 45 | 45 |
| TNS stop band, long | 42 | 42 | 40 | 40 |

Fixture values are from `qc6init_stereo-44100.txt`, `qc6init_mono-48k.txt`,
and `qc6init_stereo-48k.txt`. Values marked "derived" follow the cited formula
but have no committed `qc6init` fixture for that exact combination.

## Appendix B — Status legend

| Status | Meaning |
|---|---|
| 1 | Matching configuration and behaviour established by evidence |
| 2 | Matching intent with known implementation differences |
| 3 | Known deviation or unresolved discrepancy |
| 4 | Not applicable or not yet implemented |
