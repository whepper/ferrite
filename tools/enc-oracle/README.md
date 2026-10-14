# M2/M3 encoder oracle

Test-only tooling. Drives the **unmodified** reference FDK AAC-LC encoder
internals frame by frame and dumps decisions, spectra, energies,
thresholds, and TNS data for comparison with the native Rust encoder:
`FDKaacEnc_BlockSwitching`/`SyncBlockSwitching`, `FDKaacEnc_Transform_Real`,
then the psy-stage mirror using the real `CalcSfbMaxScaleSpec`,
`CheckBandEnergyOptim`, `TnsDetect`/`TnsSync`/`TnsEncode`,
`CalcBandEnergyOptimLong/Short`, `SpreadingMax`, `PreEchoControl`,
`CalcBandNrgMSOpt`, `groupShortData`, and `LdDataVector`. Init uses the
real `InitPsyConfiguration`/`InitTnsConfiguration`/`InitPreEchoControl`.
No production file is changed; this directory is not part of any
build system.

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
```

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
