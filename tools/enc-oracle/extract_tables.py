#!/usr/bin/env python3
"""Extract SFB width tables and TNS coefficient tables from the FDK C++ ROM
into Rust source (rust/src/aac_enc/sfb_tables.rs + tns tables).

Tables are data, not algorithms: scripted extraction avoids transcription
errors. Every emitted table is verified (counts match, widths sum to the
transform length, values match the anchors in the migration spec).

Usage: python3 tools/enc-oracle/extract_tables.py [options] [configs-transcript]   (repo root)
Writes: rust/src/aac_enc/sfb_tables.rs  (GEN=1 keeps it reproducible)

Options (all CWD-relative defaults, preserved for repo-root runs):
  --rom PATH         FDK ROM source (default: cpp/libAACenc/src/aacEnc_rom.cpp)
  --tns-src PATH     TNS source (default: cpp/libAACenc/src/aacenc_tns.cpp)
  --fdk-tools-rom PATH  FDK tools ROM (default: cpp/libFDK/src/FDK_tools_rom.cpp)
  --out PATH         output Rust file (default: rust/src/aac_enc/sfb_tables.rs)
  --header-src PATH  license/header source (default: rust/src/aac_enc.rs)
"""
import re
import sys

ROM = "cpp/libAACenc/src/aacEnc_rom.cpp"
TNS_SRC = "cpp/libAACenc/src/aacenc_tns.cpp"
FDK_TOOLS_ROM = "cpp/libFDK/src/FDK_tools_rom.cpp"
OUT = "rust/src/aac_enc/sfb_tables.rs"
HEADER_SRC = "rust/src/aac_enc.rs"

_ARGS = sys.argv[1:]
while _ARGS and _ARGS[0].startswith("--"):
    _opt = _ARGS[0]
    if _opt in ("--rom", "--tns-src", "--fdk-tools-rom", "--out", "--header-src"):
        if len(_ARGS) < 2:
            raise SystemExit(
                f"usage: extract_tables.py [--rom PATH] [--tns-src PATH] "
                f"[--fdk-tools-rom PATH] [--out PATH] [--header-src PATH] "
                f"[configs-transcript]   (repo root)")
        if _opt == "--rom":
            ROM = _ARGS[1]
        elif _opt == "--tns-src":
            TNS_SRC = _ARGS[1]
        elif _opt == "--fdk-tools-rom":
            FDK_TOOLS_ROM = _ARGS[1]
        elif _opt == "--out":
            OUT = _ARGS[1]
        else:
            HEADER_SRC = _ARGS[1]
        _ARGS = _ARGS[2:]
    elif _opt.startswith("--rom="):
        ROM = _opt[len("--rom="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--tns-src="):
        TNS_SRC = _opt[len("--tns-src="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--fdk-tools-rom="):
        FDK_TOOLS_ROM = _opt[len("--fdk-tools-rom="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--out="):
        OUT = _opt[len("--out="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--header-src="):
        HEADER_SRC = _opt[len("--header-src="):]
        _ARGS = _ARGS[1:]
    else:
        raise SystemExit(f"unknown flag {_opt}; usage: extract_tables.py "
                         f"[--rom PATH] [--tns-src PATH] [--fdk-tools-rom PATH] "
                         f"[--out PATH] [--header-src PATH] [configs-transcript]")

if len(_ARGS) > 1:
    raise SystemExit("usage: extract_tables.py [--rom PATH] [--tns-src PATH] "
                     "[--fdk-tools-rom PATH] [--out PATH] [--header-src PATH] "
                     "[configs-transcript]   (repo root)")
if _ARGS:
    CONFIGS = _ARGS[0]
else:
    CONFIGS = None

RATES = [8000, 11025, 12000, 16000, 22050, 24000, 32000,
         44100, 48000, 64000, 88200, 96000]

rom = open(ROM).read()
tns = open(TNS_SRC).read()


def get_array(src, name):
    m = re.search(
        name + r"\s*(\[[^\]]*\])?\s*=\s*\{([^;]*)\};", src, re.S)
    assert m, f"array {name} not found"
    body = m.group(2)
    # strip C comments
    body = re.sub(r"/\*.*?\*/", " ", body, flags=re.S)
    return body


def parse_sfb(name):
    body = get_array(rom, name)
    nums = [int(x) for x in re.findall(r"-?\d+", body)]
    cnt, widths = nums[0], nums[1:]
    assert len(widths) == cnt, f"{name}: cnt {cnt} != {len(widths)} values"
    return cnt, widths


def parse_floats(src, name, expect):
    body = get_array(src, name)
    nums = [float(x) for x in re.findall(r"-?\d+\.\d+", body)]
    assert len(nums) == expect, f"{name}: {len(nums)} != {expect}"
    return nums


tables = {}
for rate in RATES:
    for kind, win in (("long_1024", 1024), ("short_128", 128)):
        name = f"p_FDKaacEnc_{rate}_{kind}"
        cnt, widths = parse_sfb(name)
        assert sum(widths) == win, f"{name}: sum {sum(widths)} != {win}"
        tables[(rate, kind)] = (cnt, widths)

# TNS coefficient tables are hex Q31 patterns wrapped in
# FX_DBL2FXCONST_LPC(x) = saturating round-to-Q15: (x>>15 + 1)>>1
# (arithmetic shifts; common_fix.h). ACF windows are raw Q31 hex.
def parse_hex_list(src, name, expect):
    m = re.search(name + r"\[[^\]]*\]\s*=\s*\{([^}]*)\}", src, re.S)
    assert m, f"array {name} not found"
    vals = [int(x, 16) for x in re.findall(r"0x[0-9a-fA-F]+", m.group(1))]
    assert len(vals) == expect, f"{name}: {len(vals)} != {expect}"
    return vals


def q31_to_signed(h):
    return h - 0x100000000 if h >= 0x80000000 else h


def to_q15(h):
    v = q31_to_signed(h)
    r = ((v // 32768) + 1) // 2  # arithmetic >>15, +1, arithmetic >>1
    if v > 0 and r > 32767:
        r = 32767
    if v < 0 and r < -32768:
        r = -32768
    return r


enc3 = [to_q15(h) / 32768.0 for h in parse_hex_list(rom, "FDKaacEnc_tnsEncCoeff3", 8)]
bor3 = [to_q15(h) / 32768.0 for h in parse_hex_list(rom, "FDKaacEnc_tnsCoeff3Borders", 8)]
enc4 = [to_q15(h) / 32768.0 for h in parse_hex_list(rom, "FDKaacEnc_tnsEncCoeff4", 16)]
bor4 = [to_q15(h) / 32768.0 for h in parse_hex_list(rom, "FDKaacEnc_tnsCoeff4Borders", 16)]
acf_long_q31 = [q31_to_signed(h)
                for h in parse_hex_list(tns, "acfWindowLong", 16)]
acf_short_q31 = [q31_to_signed(h)
                 for h in parse_hex_list(tns, "acfWindowShort", 8)]
acf_long = [v / 2147483648.0 for v in acf_long_q31]
acf_short = [v / 2147483648.0 for v in acf_short_q31]

# Spot anchors (exact Q15 values verified against the C compiler, see
# tnscheck: enc3[0]=-32270, enc4[15]=32588, bor4[9]=3425)
assert tables[(48000, "long_1024")][0] == 49
assert tables[(48000, "short_128")][0] == 14
assert tables[(8000, "long_1024")][0] == 40
assert tables[(32000, "long_1024")][0] == 51
assert abs(enc3[0] - -32270 / 32768.0) < 1e-12
assert enc4[8] == 0.0
assert abs(bor4[0] - -1.0) < 1e-4  # 0x80000001 saturates near -1.0
assert abs(acf_long[0] - 1.0) < 1e-9 and abs(acf_short[0] - 1.0) < 1e-9

L = []
L.append("// @generated by tools/enc-oracle/extract_tables.py — do not edit.")
L.append("// Source data: cpp/libAACenc/src/aacEnc_rom.cpp (SFB widths, TNS")
L.append("// coefficients) and cpp/libAACenc/src/aacenc_tns.cpp (ACF windows).")
L.append("// Tables are verbatim data; see sfb.rs for selection and use.")
L.append("")
L.append("/// SFB widths for one sample rate: (long count, long widths, short count, short widths).")
L.append("pub(crate) struct SfbWidths {")
L.append("    pub(crate) sample_rate: u32,")
L.append("    pub(crate) long: &'static [u8],")
L.append("    pub(crate) short: &'static [u8],")
L.append("}")
L.append("")
for rate in RATES:
    lcnt, lw = tables[(rate, "long_1024")]
    scnt, sw = tables[(rate, "short_128")]
    L.append(f"const LONG_{rate}: [u8; {lcnt}] = [{', '.join(map(str, lw))}];")
    L.append(f"const SHORT_{rate}: [u8; {scnt}] = [{', '.join(map(str, sw))}];")
L.append("")
L.append("pub(crate) const SFB_TABLES: [SfbWidths; 12] = [")
for rate in RATES:
    lcnt, _ = tables[(rate, "long_1024")]
    scnt, _ = tables[(rate, "short_128")]
    assert lcnt == len(tables[(rate, "long_1024")][1])
    L.append(f"    SfbWidths {{ sample_rate: {rate}, long: &LONG_{rate}, short: &SHORT_{rate} }},")
L.append("];")
L.append("")
L.append("/// TNS dequantized PARCOR values, coefRes 3 (index -4..3).")
L.append(f"pub(crate) const TNS_ENC_COEF_3: [f64; 8] = [{', '.join(repr(v) for v in enc3)}];")
L.append("/// TNS quantization borders, coefRes 3.")
L.append(f"pub(crate) const TNS_COEF_3_BORDERS: [f64; 8] = [{', '.join(repr(v) for v in bor3)}];")
L.append("/// TNS dequantized PARCOR values, coefRes 4 (index -8..7).")
L.append(f"pub(crate) const TNS_ENC_COEF_4: [f64; 16] = [{', '.join(repr(v) for v in enc4)}];")
L.append("/// TNS quantization borders, coefRes 4.")
L.append(f"pub(crate) const TNS_COEF_4_BORDERS: [f64; 16] = [{', '.join(repr(v) for v in bor4)}];")
L.append("/// TNS autocorrelation window, long (16 taps, Q31).")
L.append(f"pub(crate) const TNS_ACF_LONG: [i32; 16] = [{', '.join(str(v) for v in acf_long_q31)}];")
L.append("/// TNS autocorrelation window, short (8 taps, Q31).")
L.append(f"pub(crate) const TNS_ACF_SHORT: [i32; 8] = [{', '.join(str(v) for v in acf_short_q31)}];")
L.append("")
L.append("/// Inverse-square-root table for fixed-point TNS autocorrelation")
L.append("/// normalization (`invSqrtNorm2`, 130 Q31 entries, verbatim).")
invs_body = re.search(r"invSqrtTab\[[^\]]*\]\s*=\s*\{([^}]*)\}",
                       open(FDK_TOOLS_ROM).read(), re.S)
assert invs_body, "invSqrtTab not found"
# Strip C comments: the trailing 0x3FC05F61 is commented out (130 live).
invs_body = re.sub(r"/\*.*?\*/", "", invs_body.group(1), flags=re.S)
invs = [q31_to_signed(int(x, 16))
        for x in re.findall(r"0x[0-9a-fA-F]+", invs_body)]
assert len(invs) == 130, len(invs)
assert invs[0] == 0x5A827999
L.append(f"pub(crate) const INV_SQRT_TAB: [i32; 130] = [{', '.join(str(v) for v in invs)}];")
L.append("")

# Read and validate the license/header text BEFORE opening OUT, so a
# missing header source or anchor fails without truncating the output.
anchor = "----------------------------------------------------------------------------- */"
_parts = open(HEADER_SRC).read().split(anchor)
assert len(_parts) == 2, f"header anchor missing in {HEADER_SRC}"
header = _parts[0] + anchor + "\n"
with open(OUT, "w") as f:
    # Reuse the standard FDK license header so the generated data file
    # carries the same provenance notice as every other source file.
    f.write(header)
    f.write("\n".join(L))

if CONFIGS is not None:
    # Spreading factors are init-time constants depending only on
    # (rate, long/short) for the VBR-5 target (the only bitrate branch,
    # <=20000 bit/s, never fires). Extract exact Q31 vectors from the
    # --configs dump; assert pch-invariance.
    import collections
    seen = collections.defaultdict(dict)
    cur_pch, cur_sr, cur_kind = None, None, None
    cur = {}
    for ln in open(CONFIGS):
        ln = ln.strip()
        if ln.startswith("# pch="):
            cur_pch = int(ln.split("=")[1])
        elif ln.startswith("CFG "):
            p = ln.split()
            cur_kind, cur_sr = p[1], int(p[2].split("=")[1])
            cur = {}
        elif ln.split(" ", 1)[0] in ("MASKHI", "MASKLO", "MASKHIS", "MASKLOS"):
            tag, rest = ln.split(" ", 1)
            cur[tag] = [int(x) for x in rest.split()]
            if len(cur) == 4:
                key = (cur_sr, cur_kind)
                assert key not in seen or True
                if key in seen[cur_pch]:
                    assert seen[cur_pch][key] == cur, f"pch variance {key}"
                seen[cur_pch][key] = dict(cur)
    assert len(seen[96000]) == 24 and len(seen[112000]) == 24
    for key in seen[96000]:
        assert seen[96000][key] == seen[112000][key], f"pch variance {key}"
    with open(OUT, "a") as f:
        f.write("\n/// Spreading factors as stored (exact Q31).\n")
        f.write("pub(crate) struct SpreadFactors {\n")
        f.write("    pub(crate) sample_rate: u32,\n")
        f.write("    pub(crate) hi: &'static [i32],\n")
        f.write("    pub(crate) lo: &'static [i32],\n")
        f.write("    pub(crate) hi_spr: &'static [i32],\n")
        f.write("    pub(crate) lo_spr: &'static [i32],\n")
        f.write("}\n")
        for kind in ("LONG", "SHORT"):
            for rate in RATES:
                d = seen[96000][(rate, kind)]
                n = len(d["MASKHI"])
                for tag, arr in (("HI", "MASKHI"), ("LO", "MASKLO"),
                                 ("HIS", "MASKHIS"), ("LOS", "MASKLOS")):
                    vals = d[arr]
                    assert len(vals) == n, (rate, kind, tag)
                    f.write(f"const SPREAD_{tag}_{kind}_{rate}: [i32; {n}] = "
                            f"[{', '.join(map(str, vals))}];\n")
            f.write(f"pub(crate) const SPREAD_{kind}: [SpreadFactors; 12] = [\n")
            for rate in RATES:
                n = len(seen[96000][(rate, kind)]["MASKHI"])
                f.write(f"    SpreadFactors {{ sample_rate: {rate}, "
                        f"hi: &SPREAD_HI_{kind}_{rate}, "
                        f"lo: &SPREAD_LO_{kind}_{rate}, "
                        f"hi_spr: &SPREAD_HIS_{kind}_{rate}, "
                        f"lo_spr: &SPREAD_LOS_{kind}_{rate} }},\n")
            f.write("];\n")
    print(f"appended spreading factors for 24 configs")
print(f"wrote {OUT}: {len(tables)} SFB tables, TNS tables ok")
