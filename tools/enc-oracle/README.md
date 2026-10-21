# M2/M3/M4-S1/M4-S2/M4-S3/M4-S4/M4-S5/M4-S6/M4-S7 encoder oracle

Test-only tooling. Drives the **unmodified** reference FDK AAC-LC encoder
internals frame by frame and dumps decisions, spectra, energies,
thresholds, and TNS data for comparison with the native Rust encoder:
`FDKaacEnc_BlockSwitching`/`SyncBlockSwitching`, `FDKaacEnc_Transform_Real`,
then the psy-stage mirror using the real `CalcSfbMaxScaleSpec`,
`CheckBandEnergyOptim`, `TnsDetect`/`TnsSync`/`TnsEncode`,
`CalcBandEnergyOptimLong/Short`, `SpreadingMax`, `PreEchoControl`,
`CalcBandNrgMSOpt`, `groupShortData`, and `LdDataVector`, plus the M4-S1
stage (stereo decisions via the real `PnsDetect`/`IntensityStereoProcessing`/
`MsStereoProcessing`/`CodePnsChannel`, then the real `CalcFormFactor`,
`peCalculation` and `ChannelElementWrite(NULL)`) on per-frame copies,
plus the M4-S2 stage (the real `FDKaacEnc_AdjustThresholds` VBR branch
on aliased S1 post-state copies, with the persistent `ATS_ELEMENT`
initialized once by the real `FDKaacEnc_AdjThrInit`), plus the M4-S3
stage (the real `FDKaacEnc_QuantizeSpectrum` over a gain grid plus
per-band `FDKaacEnc_calcSfbDist` /
`FDKaacEnc_calcSfbQuantEnergyAndDist` on the S1 spectra with
inactive-band lines zeroed), plus the M4-S4 stage (the real
`FDKaacEnc_EstimateScaleFactors` at `invQuant = 0/1/2` on the S1
spectra plus S2 thresholds, with inactive-band lines zeroed), plus the
M4-S5 stage (the real `FDKaacEnc_dynBitCount` per channel on fresh S4
inv=2 outputs with an inline QMAX replica and a manually wired
`BITCNTR_STATE`), plus the M4-S6 stage (the QC gain loop and VBR
rate-control closure: real `QCInit`/`AdjustBitrate`/`DistributeBits`/
`QuantizeSpectrum`/`dynBitCount`/`ChannelElementWrite`/`updateFillBits`/
`updateBitres` calls with documented inline replicas of the file-static
helpers, persistent QC/ATS state across frames), plus the M4-S7 stage (the
real `FDKaacEnc_codeValues`/`FDKaacEnc_codeScalefactorDelta` encode half
through a real writer bitstream on the saved S5 first pass, with the
file-static `encodeSpectralData`/`encodeScaleFactorData`/
`encodeSectionData` traversals replicated verbatim and self-checked
bit-exactly against the S5 counts).
Init uses the real `InitPsyConfiguration`/`InitTnsConfiguration`/
`InitPreEchoControl`. No production file is changed; this directory is not
part of any build system.

## Build

From the repository root, with the reference build already configured (M1):

```sh
BLD=/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/fdk-aac-build
c++ -std=c++11 -O2 \
  -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include \
  -I cpp/libMpegTPEnc/include -I . \
  tools/enc-oracle/enc_oracle.cpp wavreader.c \
  -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" \
  -o "$BLD/enc_oracle"
```

## Fixtures and runs

```sh
python3 tools/enc-oracle/make_fixtures.py "$BLD"
for f in m2-onset-stereo-48k m2-stepdown-stereo-48k m2-click-mono-48k \
         m2-edge-mono-48k m2-silence-stereo-48k m2-sine-stereo-48k \
         m2-impulse-mono-48k m3-multitone-stereo-48k m3-sweep-stereo-48k \
         m3-noise-mono-48k m3-tonal-noise-stereo-48k \
         m3-quiet-sine-stereo-48k m3-sine-stereo-441k m3-onset-stereo-441k; do
  "$BLD/enc_oracle" "$BLD/$f.wav" "$BLD/$f.oracle.txt"
done
# Init-time config for all 12 LC rates x both VBR-5 operating points:
"$BLD/enc_oracle" --configs > "$BLD/configs-all.txt"
# Committed testdata:
python3 tools/enc-oracle/extract_testdata.py "$BLD" "$BLD/testdata"
python3 tools/enc-oracle/extract_tables.py   # regenerates sfb_tables.rs
# Committed S1 testdata (M4-S1 selection; run the oracle first as above,
# outputs are `<stem>.s1.txt` when the binary is built from the current
# enc_oracle.cpp — the S1 stage always runs):
python3 tools/enc-oracle/extract_s1.py "$BLD" "$BLD/s1test"
# S1 re-validation (3 fresh runs; every committed S1 output line must be
# byte-identical, SPEC wobble confined to FF-inactive bands):
for d in "$BLD/v0" "$BLD/v1" "$BLD/v2"; do
  mkdir -p "$d"
  for f in m2-silence-stereo-48k m2-sine-stereo-48k m2-onset-stereo-48k \
           m2-stepdown-stereo-48k m2-click-mono-48k m2-impulse-mono-48k \
           m2-edge-mono-48k m3-multitone-stereo-48k m3-sweep-stereo-48k \
           m3-noise-mono-48k m3-tonal-noise-stereo-48k \
           m3-quiet-sine-stereo-48k m3-sine-stereo-441k m3-onset-stereo-441k \
           m6-stress-stereo-48k; do
    "$BLD/enc_oracle" "$BLD/$f.wav" "$d/$f.s1.txt"
  done
done
python3 tools/enc-oracle/validate_s1.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Committed S2 testdata (M4-S2 selection; same dumps, S2FRAME..S2END
# blocks parsed by extract_s2.py):
python3 tools/enc-oracle/extract_s2.py "$BLD" "$BLD/s2test"
# S2 re-validation (every committed S2 line byte-identical; no wobble is
# expected — S2 inputs derive from S1 vectors plus deterministic config):
python3 tools/enc-oracle/validate_s2.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Committed S3 testdata (M4-S3 selection; same dumps, S3FRAME..S3END
# blocks parsed by extract_s3.py; gains -72/-32/-8/64):
python3 tools/enc-oracle/extract_s3.py "$BLD" "$BLD/s3test"
# S3 re-validation (every committed S3 line byte-identical, strict —
# inactive-band scratch is zeroed at the S3 input, so no wobble can
# leak in):
python3 tools/enc-oracle/validate_s3.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Committed S4 testdata (M4-S4 selection; same dumps, S4FRAME..S4END
# blocks parsed by extract_s4.py; invQuant 0/1/2 levels):
python3 tools/enc-oracle/extract_s4.py "$BLD" "$BLD/s4test"
# S4 re-validation (every committed S4 line byte-identical, strict —
# same zeroing contract as S3, so no wobble can leak in):
python3 tools/enc-oracle/validate_s4.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Committed S5 testdata (M4-S5 selection; same dumps, S5FRAME..S5END
# blocks parsed by extract_s5.py; S4 inv=2 outputs as inputs):
python3 tools/enc-oracle/extract_s5.py "$BLD" "$BLD/s5test"
# S5 re-validation (every committed S5 line byte-identical, strict —
# S4 inv=2 inputs are deterministic per S4-STABLE, so no wobble):
python3 tools/enc-oracle/validate_s5.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Committed S6 testdata (M4-S6 selection; same dumps, S6FRAME..S6END
# blocks plus the per-file S6INIT line, parsed by extract_s6.py):
python3 tools/enc-oracle/extract_s6.py "$BLD" "$BLD/s6test"
# S6 re-validation (every committed S6 line byte-identical, strict —
# S4/S5 inputs are deterministic per S4/S5-STABLE and QC/ATS state replays
# identically, so no wobble):
python3 tools/enc-oracle/validate_s6.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Committed S7 testdata (M4-S7 selection; same dumps, S7FRAME..S7END
# blocks parsed by extract_s7.py; S5 first-pass outputs as inputs):
python3 tools/enc-oracle/extract_s7.py "$BLD" "$BLD/s7test"
# S7 re-validation (every committed S7 line byte-identical, strict —
# S5 first-pass inputs are deterministic per S5-STABLE, so no wobble):
python3 tools/enc-oracle/validate_s7.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
# Synthetic quantizer probes (no WAV needed; envelope-gated transcripts
# for Rust anchors) plus scripted table extraction:
c++ -std=c++11 -O2 \
  -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include \
  -I cpp/libMpegTPEnc/include -I . \
  tools/enc-oracle/quant_probe.cpp \
  -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" \
  -o "$BLD/quant_probe"
"$BLD/quant_probe" > "$BLD/qprobe.txt"
python3 tools/enc-oracle/extract_quant_tables.py "$BLD/qprobe.txt"
```

### Config-only regeneration

Config snapshots can be regenerated without rewriting any committed per-frame
vector (`seq_`, `mdct_`, `ring_`, `psy_`, `rxx_`, `ms_`):

```sh
mkdir -p "$BLD/cfg"
python3 tools/enc-oracle/extract_testdata.py --cfg-only "$BLD" "$BLD/cfg"
```

The built-in sources cover 48000/96000, 48000/112000, and 44100/96000. The
mono 44.1 kHz snapshot additionally needs the M5.b input dump and an explicit
config stem:

```sh
# Writes $TMPDIR/m5b-quality/inputs/onset-mono-44100.orig.wav
(cd rust && cargo test --features aac_enc m5b_dump_inputs)
"$BLD/enc_oracle" "$TMPDIR/m5b-quality/inputs/onset-mono-44100.orig.wav" \
  "$BLD/oracle/onset-mono-44100.oracle.txt"
python3 tools/enc-oracle/extract_testdata.py --cfg-only \
  "$BLD/oracle" "$BLD/cfg" onset-mono-44100
```

Extra stems are additive. Expected outputs are byte-identical to the committed
`rust/src/aac_enc/testdata/cfg_*.txt` files (M5.e/M6.4 record the hashes).

## Output format

Header line `# sr=.. ch=.. frames=.. src=.. chBitrate=..`, config blocks
(`CFG LONG/SHORT` + `OFFSETS`/`PCMQ`/`MASKHI`/`MASKLO`/`MASKHIS`/`MASKLOS`/
`MINSNR`, `TNSCFG LONG/SHORT`), then per frame and channel:

```
FRAME <f> CH <c> SEQ <seq> SHAPE <shape> GROUPS <n> <g0> <g1> <g2> <g3>
RING <2048 int16>
MDCT_E <exponent>
MDCT <1024 int32, pre-TNS rescaled spectrum>
RXX1 ch=<c> w=<w> <13 Q31 ints>   (per window; ground-truth merged
RXX2 ch=<c> w=<w> <13 Q31 ints>    autocorrelation, same detect input)
PSY mdctScale=<m> finalShift=<f> tnsShift=<k> nrgScaling=<kc>
MAXSCALE <sfbActive ints>
NRG <E ints> | NRG<w> ... (short: per window)
NRGLD / THR / SPREAD (same layout)
TNSW w=<w> nf=<n> ord=<o0,o1> len=<l0,l1> dir=<d0,d1> coefs=<12 HI> | <12 LO>
TNSG w=<w> active=<a0,a1> gain=<g0,g1> merged=<m>
NM1 <sfbActive ints>
NM1STATE m1=<mdctScalenm1> calc=<calcPreEcho>
GROUPED maxSfb=<m> noSfb=<n> + GE/GT/GS/GELD/GTLD/GOFF/GMSNR (short only)
LONGOUT maxSfb=<m> + TLD (long only)
```

`SEQ`: 0 LONG, 1 START, 2 SHORT, 3 STOP (`psy_const.h`). `SHAPE`: 0 SINE,
1 KBD. MDCT/NRG/THR values are stored-domain integers; energies carry the
`finalShift`/`tnsSpecShift` scalings with scale-corrected `NRGLD`
(`log2/64`, `-1.0` sentinel for zero). `RING` is the 2048-sample transform
input after the 448-sample pre-copy (`psy_main.cpp`), i.e. before rotation.
Three all-zero flush frames are appended after the input
(cf. `aacenc_lib.cpp` flush behavior).

## S1 stage output format

Per frame, after the M2/M3 dumps, one `S1FRAME f=<f>` block with the exact
Q31 inputs the real C S1 functions consumed plus their outputs:

```
S1FRAME f=<f>
ATS pre=<e0,e1,l0,l1>     (weighting state entering the frame)
ATS post=<e0,e1,l0,l1>    (weighting state leaving the frame)
S1GEO ch=<c> cnt=<n> per=<p> max=<m> seq=<s>
SOFF <cnt+1 offsets>      (exact SFB offsets consumed; grouped if short)
STNSW ...                 (TNS decisions, M3 TNSW layout, all windows)
STNSR <coefRes per window>
SPEC ch=<c> <1024>        (post-MS/IS-decision spectrum)
PENLD/PTHLD <cnt>         (PE input energy/threshold Ld)
SENL <cnt>                (PE input linear energies)
FF/ENFAC/WENLD/WTHLD <cnt> (form factor, EnFac, weighted energies/thresholds)
SPNL/SPECH/SPCO/SPNA <cnt> (per-band PE details)
PECH ch=<c> pe=<p> const=<q> nact=<n>
ISB/ISS <cnt>             (intensity books/scales; zeros on corpus)
NOISE <sfbActive>         (all NO_NOISE_PNS on corpus, usePns=0)
MSD <digest> <mask...>    (stereo only)
PE <pe> <const> <nact> <offset>
STATIC <SCE|CPE> <bits>
```

`SPEC` tails on short frames may hold C scratch values past the grouped
data (same pad story as M3 `GSPEC`); the S1 functions only read bands
below `maxSfbPerGroup`, and `validate_s1.py` proves the wobble never
touches those bands on committed frames. `ATS pre` is injected by Rust
tests so no cross-frame history replay is needed; `ATS post` validates
the state evolution.

## S2 stage output format

Per frame, after the S1FRAME block, one `S2FRAME f=<f>` block with the
exact Q31 inputs the real `FDKaacEnc_AdjustThresholds` VBR branch
consumed plus its outputs:

```
S2FRAME f=<f>
S2CHAOS pre=<old> post=<old'>   (persistent chaosMeasureOld evolution)
S2GEO ch=<c> cnt=<n> per=<p> max=<m> seq=<s>
GROUPLEN <4 groupLens>          (short-block grouping; long blocks: zeros)
THREXP <cnt>                    (calcThreshExp via real CalcInvLdData)
MINSNR0 <cnt>                   (minSnr input: grouped config / static cfg)
SPREAD0 <cnt>                   (post-MS spread input)
THRV <cnt>                      (final thresholds: VBR reduce + enFac add)
MINSNR1 <cnt>                   (minSnr after adaptMinSnr + avoid-hole init)
SPREAD1 <cnt>                   (spread after avoid-hole init)
S2END
```

Buffer roles mirror the aliased encoder (`aacenc.cpp:803-808`): S2
threshold inputs are the post-weighting S1 vectors (`WTHLD`), energies
the unweighted `SENL`/`PENLD` vectors. The VBR path never writes psy
arrays, so the oracle's separate (identically-initialized) copies are
numerically identical to the aliased buffers. `thrExp`/`ahFlag`
scratch past the active bands is uninitialized in C and unread
everywhere (all loops bounded by `maxSfbPerGroup`); the Rust port reads
0 there. `S2CHAOS pre` is injected by Rust tests (with the onset
f22–f26 consecutive run proving multi-frame trajectory continuity);
`post` validates the evolution.

## S3 stage output format

Per frame, after the S2FRAME block, one `S3FRAME f=<f>` block with the
real `FDKaacEnc_QuantizeSpectrum` outputs over the gain grid
`[-72, -32, -8, 64]` (uniform QSS: `scf = 0`, `globalGain = gain`,
`dZoneQuantEnable = 0`) plus per-band distortion probes:

```
S3FRAME f=<f>
S3GEO ch=<c> cnt=<n> per=<p> max=<m> seq=<s>
S3OFF <cnt+1 offsets>      (same geometry as S1 SOFF, cross-checked)
S3Q g=<gain> ch=<c> <1024> (full quantized spectrum; bands past
                            maxSfbPerGroup read 0 — QuantizeSpectrum
                            never visits them and the buffer is zeroed)
S3DIST g=<gain> ch=<c> <cnt> (per-band calcSfbDist Ld outputs)
S3EN g=<gain> ch=<c> en <cnt> dist <cnt>
                           (per-band calcSfbQuantEnergyAndDist with the
                           S3Q lines fed back in)
S3END
```

S3 input contract: the S1 post-MS/IS spectrum with inactive-band
lines (per-band index `>= maxSfbPerGroup`) zeroed before any call.
Those lines hold proven C scratch — `validate_s1.py` confines every
run-to-run SPEC difference to exactly those bands — and no downstream
consumer ever reads them (every reference loop is bounded by
`maxSfbPerGroup`, including the S4 scf search and S5/S6 counting).
Zeroing keeps S3DIST/S3EN deterministic; the Rust tests zero the same
lines (`s3_input_spec` in `quant.rs`).

Valid-envelope gate: every S3 line is scanned for the reference's own
assert conditions before the calls (`totalShift >= 0` per line,
no `SHORT_MIN` in any S3Q vector); a violation aborts the oracle
loudly instead of executing undefined shifts or out-of-bounds table
reads (`quant_probe.cpp` documents the same gate for synthetic
probes). Committed fixtures are therefore valid on every target,
including x86 where `FDK_ASSERT` compiles in.

## Synthetic quantizer probes (`quant_probe.cpp`)

`QL` (single-line `QuantizeSpectrum`: magnitudes × gains × dead zones),
`QBANDS` (2-group scf ramp), `QD` (`calcSfbDist` bands incl. the
`MAX_QUANT` early-out sweep), `QI` (`calcSfbQuantEnergyAndDist` with
caller-chosen lines incl. ±8191/±8192 and mid-band clipping). The
`QPROBE tables` block prints the exact build integers for scripted
extraction (`extract_quant_tables.py` → `rust/src/aac_enc/
quant_tables.rs`), cross-checked against the `aacEnc_rom.cpp` source
forms. The transcript is committed as
`rust/src/aac_enc/testdata/qprobe_quant.txt` and replayed exactly by
the Rust tests.

## S4 stage output format

Per frame, after the S2FRAME block (inside the S2 scope, so S1 and S2
inputs are both visible), one `S4FRAME f=<f>` block with the real
`FDKaacEnc_EstimateScaleFactors` outputs at three `invQuant` levels on
identical inputs (`0` = initial formula only, `1` = +`improveScf` and
single assimilation, `2` = full afterburner). Inputs mirror the aliased
encoder: spectrum = S1 post-MS/IS spectrum with inactive-band lines
zeroed (S3 input contract), energies = S1 unweighted Ld (`PENLD`),
thresholds = S2 post-VBR (`THRV`), form factors = S1 (`FF`); psy/qc
pointers alias the same working spectrum buffer, exactly like
`aacenc.cpp:803-808`. `dZoneQuantEnable = 0` (LC).

```
S4FRAME f=<f>
S4GEO ch=<c> cnt=<n> per=<p> max=<m> seq=<s>
S4OFF <cnt+1 offsets>      (same geometry as S1 SOFF, cross-checked)
S4SCF inv=<0|1|2> ch=<c> <cnt>   (POST-inversion stored scf:
                                  `maxSf - orig`, dead bands `0`;
                                  bands past maxSfbPerGroup stay
                                  `FDK_INT_MIN`, never visited)
S4GAIN inv=<v> ch=<c> <gain>     (globalGain = max orig scf, `0` when all dead)
S4Q inv=<v> ch=<c> <1024>        (final quantized lines; dead bands 0)
S4SPEC inv=<v> ch=<c> <1024>     (mutated spectrum: dead bands zeroed)
S4DIST inv=<v> ch=<c> <cnt>      (per-band calcSfbDist with final
                                  `QSS = gain - scf`, diagnostic — the
                                  internal sfbDist scratch is not exposed
                                  by the exported call)
S4EN inv=<v> ch=<c> en <cnt> dist <cnt>
                                 (per-band calcSfbQuantEnergyAndDist with
                                  the S4Q lines fed back in, same QSS)
S4END
```

Valid-envelope gate like S3: any `SHORT_MIN` line or recomputed
`totalShift < 0` aborts loudly. The three levels share inputs (fresh
spectrum copies per level, since the call zeroes dead bands in place),
so `inv=0` vs `1` vs `2` localizes divergences to the initial formula,
single assimilation, or the multiple stages.

## S5 stage output format

Per frame, after the S4FRAME block (inside the S2 scope, so S1, S2 and
S4 inputs are all visible), one `S5FRAME f=<f>` block with the real
`FDKaacEnc_dynBitCount` result per channel. Inputs mirror the QCMain
gain loop (`qc_main.cpp:994-999`): quantized lines = a fresh S4 inv=2
run (deterministic per S4-STABLE, recomputed to keep stages
independent), `maxValueInSfb` = per-band max abs via an inline replica
of the S6-owned static `FDKaacEnc_calcMaxValueInSfb` (`qc_main.cpp:1219`,
zero-init for determinism — unvisited bands are never read),
scalefactors = S4 inv=2 post-inversion scf, `blockType` =
`lastWindowSequence`, geometry/`noiseNrg`/`isBook`/`isScale` from the S1
psy outputs, `syntaxFlags = 0` (encoder default, `aacenc.cpp:358`, so
`useVCB11 = 0`). `BITCNTR_STATE` is wired manually (plain struct of two
scratch arrays; no behavior change).

```
S5FRAME f=<f>
S5GEO ch=<c> cnt=<n> per=<p> max=<m> seq=<s>
S5OFF <cnt+1 offsets>      (same geometry as S1 SOFF, cross-checked)
S5QMAX ch=<c> <cnt>       (per-band max abs; unvisited bands 0;
                            range-checked 0..32768 at extraction)
S5ISB ch=<c> <cnt>        (isBook: all zero corpus-wide, S1-proven)
S5ISS ch=<c> <cnt>        (isScale: all zero corpus-wide)
S5NOISE ch=<c> <cnt>      (noiseNrg: all NO_NOISE_PNS corpus-wide)
S5SECT ch=<c> nsec=<n>
S5SEC ch=<c> i=<i> book=<b> start=<s> cnt=<k> bits=<bits>
                           (one line per section, strict order)
S5BITS ch=<c> block=<bt> nsec=<n> side=<s> huff=<h> scf=<c>
                  noise=<nz> first=<f> dyn=<d>
S5END
```

Valid-envelope gate: any `SHORT_MIN` line aborts loudly (S3/S4 input
contract re-asserted); the `FDK_ASSERT` in
`FDKaacEnc_bitCountScalefactorDelta` stays armed (build has no
`-DNDEBUG`), so any `|Δscf| > 60` aborts the oracle instead of
OOB-reading — the run completing is the envelope proof.

## S5 tables (`bitcount_probe.cpp`, `extract_bitcount_tables.py`)

`bitcount_probe` prints the eight integer tables `dyn_bits.cpp` /
`bit_cnt.cpp` consume (`huff_ltab1_2/3_4/5_6/7_8/9_10/11`,
`sideInfoTabLong/Short`) as the exact build integers.
`extract_bitcount_tables.py` writes
`rust/src/aac_enc/bitcount_tables.rs` and cross-checks every value
against an independent parse of `cpp/libAACenc/src/aacEnc_rom.cpp`
(all eight tables are plain hex/decimal initializers, so source text
is exact) — any mismatch aborts. The scf-DPCM table is NOT duplicated:
S5 reuses `pe::bit_count_scf_delta`.

## S6 stage output format

Once per file, before the frame loop, one `S6INIT` line with the QC-init
vectors from the real `FDKaacEnc_QCInit` path (which calls the real
`InitElementBits` + `AdjThrInit`):

```
S6INIT avg=<averageBits> max=<maxBits> min=<minBits> bitres=<bitRes>
  mode=<bitResMode 0=FULL> distmode=<bitDistributionMode>
  meanpe=<meanPe> maxbitfac=<maxBitFac> chbitrateEl=<..> maxbitsEl=<..>
  relbitsEl=<..> bits2pe=<m,e> pemin=<..> pemax=<..>
```

Per frame, after the S5FRAME block (same scope, so S1/S2/S4/S5 inputs are
all visible), one `S6FRAME f=<f>` block. Grants and the bit budget derive
from `maxBitsPerFrame` at VBR (QCMain passes it — not the frame average —
into `BitResRedistribution`/`prepareBitDistribution`,
`qc_main.cpp:805-806,836-839`); `AdjustBitrate` only evolves
`paddingRest`. `globHdr` is the transport-probed ADTS constant 56
(`adtsWrite_GetHeaderBits` with `protection_absent = 1`, PCE absent for
SCE/CPE; verified with a real `transportEnc` probe: 56/56/56
independent of `auBits`).

```
S6FRAME f=<f>
S6BITR avgTotal=<avgTotalBits> globHdr=56 padRest=<paddingRest>
S6GRANT granted=<grantedDynBits> maxdyn=<maxDynBits> avail=<totalAvail>
  elgrant=<el> gpe=<grantedPe> gpecorr=<grantedPeCorr>
  reslevel=<bitResLevelEl> resmax=<maxBitResBitsEl>
S6GSTATE <peLast dynBitsLast corrM corrE peMin peMax>  (pre-frame ATS)
S6IT pass=<k> iter=<iterations> dec=<decrease> dyn0=<..> dyn1=<..>
  gain0=<..> gain1=<..> maxq0=<..> maxq1=<..> eldyn=<..> elstat=<..>
  crash=<-1|surviving maxSfbPerGroup>
  (one line per outer gain-loop pass, strict pass order)
S6FINAL static=<..> useddyn=<..> totfill=<..> align=<..> total=<..>
  granted=<..> bitres=<..> crash=<..> npass=<..>
S6GPOST <same 6-tuple>  (post-frame ATS; PRE(f+1)==POST(f))
S6END
```

Iteration 0 reuses the saved S4/S5 first pass; retries re-quantize via the
real `QuantizeSpectrum` + inline `calcMaxValueInSfb` replica + real
`dynBitCount`, with inline `reduceBitConsumption`/`crashRecovery`
replicas (crash uses real `countValues` + real `ChannelElementWrite`
recount). Envelope gates: `SHORT_MIN` abort, `QUANT_ERROR` abort,
25-pass runaway abort. `S6FINAL total + 56` equals the real `aac-enc`
ADTS frame length bit-exactly on every frame where oracle inputs match
the encoder (all long windows; the known short-block blips — 10 frames
across the corpus, all `SHORT_WINDOW` — are documented in the S6 plan
record, not hidden).

## Synthetic S4 probes (`scf_probe.cpp`, `scf_unit_probe.cpp`)
`scf_probe` prints every `FL2FXCONST_DBL` literal `sf_estim.cpp` uses,
as the exact build integers (constants are data, never hand-transcribed;
the values live in `scf.rs` with the probe file cited).
`scf_unit_probe` calls the reference `sf_estim.cpp` static helpers
directly (by including the implementation file test-only; production
untouched) and prints deterministic transcripts: `CB`
(`countSingleScfBits` over a dense scf×delta grid incl. small
corpus-like deltas), `SP` (`calcSingleSpecPe` over a dense
scf×constPe×nLines grid incl. corpus values), `RL`
(`calcSfbRelevantLines` on a fixed 4-band vector), `SN` (a full
`assimilateSingleScf` run with nrel/dist/qs), `CH` (full
`EstimateScaleFactors` at `inv=0/1/2` on a 4-band vector with a dead
band), `IM` (`improveScf` trials), `N46` (`calcSfbRelevantLines` on the
corpus band-46 inputs), `QB45`/`QB46` (`calcSfbDist` on corpus bands at
their S4 QSS values — the mod-2 gains the S3 grid never covered), `SX`
(a 6-band corpus-slice single run). The transcript is committed as
`rust/src/aac_enc/testdata/scfunit.txt` and replayed exactly by the
Rust tests (`s4_unit_probe_exact`).

## S6 probes (`s6lit_probe.cpp`, `tp_probe.cpp`)

`s6lit_probe` prints every `FL2FXCONST_DBL` expression the S6 port uses
(`DistributeBits` family + bitres helpers), evaluated exactly as C
evaluates them — every literal in `rust/src/aac_enc/rate.rs` cites this
probe (pinned by `s6_q31_literals_match_probe`):

```sh
c++ -std=c++11 -O2 \
  -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include \
  -I cpp/libMpegTPEnc/include -I . \
  tools/enc-oracle/s6lit_probe.cpp \
  -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" \
  -o "$BLD/s6lit_probe"
"$BLD/s6lit_probe"
```

`tp_probe` verifies the ADTS `globHdrBits` transport constant through the
real `transportEnc` (mono/stereo ADTS: 56 bits regardless of `auBits`).
S6 takes the header bits as a transport-owned input; the probe is the
provenance of the constant:

```sh
c++ -std=c++11 -O2 \
  -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include \
  -I cpp/libMpegTPEnc/include -I . \
  tools/enc-oracle/tp_probe.cpp \
  -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" \
  -o "$BLD/tp_probe"
"$BLD/tp_probe"
```

## S7 stage output format

Per frame, after the S6FRAME block (same scope, so the saved S5 first
pass — S4 inv=2 lines/scf/gain, QMAX, sections, geometry, PNS/IS — is
all visible), one `S7FRAME f=<f>` block. The stage replicates the three
file-static payload writers verbatim
(`encodeSpectralData`/`encodeScaleFactorData`/`encodeSectionData`,
`bitenc.cpp:127-369`) but calls the REAL exported leaf functions
(`FDKaacEnc_codeValues`, `FDKaacEnc_codeScalefactorDelta`) and the real
`FDKwriteBits` through a real writer bitstream (2048-byte scratch,
`FDKsyncCache` flush, MSB-first hex). Each payload is self-checked
against the S5 counts — the production `WRITE_SEC/SCAL/SPEC_ERROR`
agreement (`bitenc.cpp:1049-1122`) — and any deviation aborts loudly:

```
S7FRAME f=<f>
S7SPEC ch=<c> bits=<n> hex=<...>   (spectral payload == huffmanBits)
S7SCF ch=<c> bits=<n> hex=<...>    (scf payload == scalefacBits+noiseNrgBits)
S7SECH ch=<c> bits=<n> hex=<...>   (section headers == sideInfoBits)
S7END
```

Zero-payload channels (15 across the corpus, e.g. sine f5 ch1) emit
`bits=0` with empty hex for `S7SPEC`/`S7SCF` but still carry section
headers. The `FDK_ASSERT` in
`FDKaacEnc_bitCountScalefactorDelta` stays armed, and every
`codeScalefactorDelta` return code is checked, so the run completing is
also proof that no corpus `|Δscf|` exceeds 60 on the encode path.

## S7 tables and vectors (`bitenc_probe.cpp`, `extract_bitenc_tables.py`)

`bitenc_probe` (no WAV needed) prints the twelve code tables the encode
half reads (`huff_ctab1/2/3/4/5/6/7/8/9/10/11`, `huff_ctabscf` — note
`ctab11` is `[21][17]`; the encoder only indexes rows/cols 0..16) as
the exact build integers, then encodes 287 synthetic vectors through the
real leaf functions: every book (incl. zero and the C default arm for
books 12/14/`-1`, which emits nothing), all signs, paired-codeword and
pair-order widths, the full book-11 escape sweep (15/16/17, 31/32/33,
63/64, …, 16383/16384, 32767 in every pair position, plus ±8191 and
`±32767`/`-32768`), and the full scalefactor sweep `-61..=61`
(`±61` pin the C `return 1` error with zero bits). The transcript is
committed as `rust/src/aac_enc/testdata/s7probe.txt` and replayed
bit-for-bit (counts + hex) by the Rust tests
(`s7_probe_transcript_matches_c_bit_exact`), which additionally assert
live S5-counter agreement on every valid vector.
`extract_bitenc_tables.py` writes
`rust/src/aac_enc/bitenc_tables.rs` and cross-checks every value
against an independent parse of `cpp/libAACenc/src/aacEnc_rom.cpp` —
any mismatch aborts. Lengths are NOT duplicated: S7 shares the S5
length tables and `pe::bit_count_scf_delta` single-source.

```sh
c++ -std=c++11 -O2 \
  -I cpp/libAACenc/src -I cpp/libFDK/include -I cpp/libSYS/include \
  -I cpp/libMpegTPEnc/include -I . \
  tools/enc-oracle/bitenc_probe.cpp \
  -L "$BLD" -lfdk-aac -Wl,-rpath,"$BLD" \
  -o "$BLD/bitenc_probe"
"$BLD/bitenc_probe" > "$BLD/s7probe.txt"
python3 tools/enc-oracle/extract_bitenc_tables.py "$BLD/s7probe.txt"
```

## S8 element assembly and raw blocks (`extract_s8.py`, `validate_s8.py`)

Per frame, after the S6FRAME block, one `S8FRAME f=<f>` block. The stage
assembles the complete element and raw-data block from the saved S5
first pass (S4 inv=2 lines/scf/gain, QMAX, sections, geometry, PNS/IS —
the final state on the single-pass corpus) plus the saved S1 side info
(`windowShape`/`groupingMask`/`mdctScale`/TNS/MS) and the saved S6
fill/align/total. Static scaffolding (ICS/MS/gain/pulse/TNS) uses direct
`FDKwriteBits` (fixed syntax, counts proven by S1); section/scale/
spectral use the REAL `FDKaacEnc_codeValues`/
`FDKaacEnc_codeScalefactorDelta` with the S7-verified traversals,
self-checked bit-exactly against the S5 counts (the production
`WRITE_SEC/SCAL/SPEC_ERROR` agreement, `bitenc.cpp:1049-1122`). Fill uses
the `EXT_FILL_DATA` loop (`bitenc.cpp:851-902`) self-checked against the
S6 sized fill; `ID_END` + `alignBits` close the block and the raw total
is checked against S6 (`bitenc.cpp:1317-1319`). `S8ELEM` is dumped before
the fill tail (zero-padded tail, like the Rust `BitWriter`); `S8RAW`
after (`ID_END` + align included):

```
S8FRAME f=<f>
S8SIDE ch=<c> wshape=<s> gmask=<m> mdct=<e>
S8ELEM bits=<n> hex=<...>   (element == static + dyn)
S8RAW bits=<n> hex=<...>    (raw block == element + fill + END + align)
S8FINAL static=<s> useddyn=<u> totfill=<t> align=<a> total=<n>
S8END
```

```sh
python3 tools/enc-oracle/extract_s8.py "$BLD" "$BLD/s8test"
python3 tools/enc-oracle/validate_s8.py "$BLD/v0" "$BLD/v1" "$BLD/v2"
```
