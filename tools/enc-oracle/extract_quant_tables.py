#!/usr/bin/env python3
"""Extract M4-S3 quantizer tables from the C probe into Rust source.

Usage: extract_quant_tables.py [--rom PATH] [--out PATH] [--header-src PATH] <qprobe-txt>   (repo root)

Options (all CWD-relative defaults, preserved for repo-root runs):
  --rom PATH         FDK ROM source (default: cpp/libAACenc/src/aacEnc_rom.cpp)
  --out PATH         output Rust file (default: rust/src/aac_enc/quant_tables.rs)
  --header-src PATH  license/header source (default: rust/src/aac_enc.rs)

Reads the QPROBE tables block of <qprobe-txt> (produced by quant_probe,
which prints the exact integers the reference build uses) and writes
rust/src/aac_enc/quant_tables.rs. Run `cargo fmt` afterwards: like
extract_tables.py, this script emits fixed chunking and relies on
rustfmt for final layout (the committed file is script output +
rustfmt).

Every value is cross-checked against an INDEPENDENT recomputation from
the C++ source text (cpp/libAACenc/src/aacEnc_rom.cpp):
 - MTAB34/QTABQ/QTABE: QTC(hex) = FX_DBL2FXCONST_SGL, i.e. 16-bit
   ((val>>15)+1)>>1 with saturation (FIXP_QTD is FIXP_SGL on the
   32x16 arches, which cover arm64 and x86 alike; aacEnc_rom.h:148-154).
 - MTAB43/EXPM: FL2FXCONST_DBL(decimal) = trunc(val*2^31 +/- 0.5).
 - KZ/KO: FL2FXCONST_DBL applied to the float32 sums (-0.0946f+0.5f)
   and (0.23f), then >>16.
Any mismatch aborts: tables are data, never hand-transcribed.
"""
import re
import struct
import sys

QPROBE = None
ROM = "cpp/libAACenc/src/aacEnc_rom.cpp"
OUT = "rust/src/aac_enc/quant_tables.rs"
HEADER_SRC = "rust/src/aac_enc.rs"

_ARGS = sys.argv[1:]
while _ARGS and _ARGS[0].startswith("--"):
    _opt = _ARGS[0]
    if _opt in ("--rom", "--out", "--header-src"):
        if len(_ARGS) < 2:
            raise SystemExit(
                "usage: extract_quant_tables.py [--rom PATH] [--out PATH] "
                "[--header-src PATH] <qprobe-txt>   (repo root)")
        if _opt == "--rom":
            ROM = _ARGS[1]
        elif _opt == "--out":
            OUT = _ARGS[1]
        else:
            HEADER_SRC = _ARGS[1]
        _ARGS = _ARGS[2:]
    elif _opt.startswith("--rom="):
        ROM = _opt[len("--rom="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--out="):
        OUT = _opt[len("--out="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--header-src="):
        HEADER_SRC = _opt[len("--header-src="):]
        _ARGS = _ARGS[1:]
    else:
        raise SystemExit(f"unknown flag {_opt}; usage: extract_quant_tables.py "
                         f"[--rom PATH] [--out PATH] [--header-src PATH] "
                         f"<qprobe-txt>   (repo root)")
if len(_ARGS) != 1:
    raise SystemExit("usage: extract_quant_tables.py [--rom PATH] [--out PATH] "
                     "[--header-src PATH] <qprobe-txt>   (repo root)")
QPROBE = _ARGS[0]

txt = open(QPROBE).read().splitlines()


def block(tag):
    for ln in txt:
        if ln.startswith(tag + " ") or ln.startswith(tag + "\n") or ln == tag:
            return ln
    # tags with values on the same line: "QTABQ v v v v"
    for ln in txt:
        if ln.startswith(tag + " "):
            return ln
    raise AssertionError(f"block {tag} missing")


def ints(tag):
    ln = block(tag)
    return [int(x) for x in ln.split()[1:]]


def to_signed32(v):
    return v - 0x100000000 if v >= 0x80000000 else v


# ---- probe values ----
arch = block("QTARCH")
assert "qtd_bytes=2" in arch, f"FIXP_QTD not 16-bit: {arch}"
assert "mant_digits=9" in arch
qtab_q = ints("QTABQ")
qtab_e = ints("QTABE")
mtab34 = ints("MTAB34")
mtab43 = ints("MTAB43")
expm = ints("EXPM")
exps = ints("EXPS")
kz_ln = block("KZ")
kz, ko = int(kz_ln.split()[1]), int(kz_ln.split()[3])
assert len(qtab_q) == 4 and len(qtab_e) == 4
assert len(mtab34) == 512 and len(mtab43) == 512
assert len(expm) == 56 and len(exps) == 56

# ---- independent recomputation from source text ----
rom = open(ROM).read()


def get_body(name):
    m = re.search(name + r"\s*(\[[^\]]*\]\s*(\[[^\]]*\])?)?\s*=\s*\{([^;]*)\};",
                  rom, re.S)
    assert m, f"{name} not found"
    return m.group(3)


def fx_sgl(val):
    # FX_DBL2FXCONST_SGL (common_fix.h:160-166): val is the raw Q31 int.
    q = ((val >> 15) + 1) >> 1
    if val > 0 and (((val >> 15) + 1) > ((1 << 16) - 1)):
        return (1 << 15) - 1
    # negative saturation cannot trigger for these tables; mirror anyway
    if val < 0 and (((val >> 15) + 1) < -(1 << 16)):
        return -(1 << 15)
    # wrap to SHORT like the C conversion
    q &= 0xFFFF
    return q - 0x10000 if q >= 0x8000 else q


def fl_dbl(x):
    # FL2FXCONST_DBL (common_fix.h:191-201) for a Python float (= double).
    if x >= 0:
        v = int(x * 2**31 + 0.5)
        return 0x7FFFFFFF if v >= 0x7FFFFFFF else v
    else:
        v = int(x * 2**31 - 0.5)  # truncates toward zero like (LONG)
        # careful: int() truncates toward zero; C (LONG) too. For
        # negatives x*2^31-0.5 is e.g. -100.7 -> (LONG) = -100.
        return -0x80000000 if v <= -0x80000000 else v


def f32(x):
    return struct.unpack("f", struct.pack("f", x))[0]


body34 = get_body("FDKaacEnc_mTab_3_4")
hex34 = [to_signed32(int(h, 16)) for h in re.findall(r"0x[0-9a-fA-F]+", body34)]
assert len(hex34) == 512, len(hex34)
exp34 = [fx_sgl(v & 0xFFFFFFFF) if v < 0 else fx_sgl(v) for v in hex34]
# fx_sgl takes the raw bit pattern: redo with unsigned for negatives
exp34 = [fx_sgl(v if v >= 0 else v + 0x100000000) for v in hex34]
assert exp34 == mtab34, "MTAB34 mismatch"


def check_qtab(name, probe):
    body = get_body(name)
    hexes = [int(h, 16) for h in re.findall(r"0x[0-9a-fA-F]+", body)]
    assert len(hexes) == 4, (name, len(hexes))
    assert [fx_sgl(v) for v in hexes] == probe, f"{name} mismatch"


check_qtab("FDKaacEnc_quantTableQ", qtab_q)
check_qtab("FDKaacEnc_quantTableE", qtab_e)

body43 = get_body("FDKaacEnc_mTab_4_3Elc")
dec43 = [float(x) for x in re.findall(r"-?\d+\.\d+", body43)]
assert len(dec43) == 512, len(dec43)
assert [fl_dbl(x) for x in dec43] == mtab43, "MTAB43 mismatch"

body_e = get_body("FDKaacEnc_specExpMantTableCombElc")
dec_e = [float(x) for x in re.findall(r"-?\d+\.\d+", body_e)]
assert len(dec_e) == 56, len(dec_e)
assert [fl_dbl(x) for x in dec_e] == expm, "EXPM mismatch"

body_s = get_body("FDKaacEnc_specExpTableComb")
exp_s = [int(x) for x in re.findall(r"\d+", body_s)]
assert exp_s == exps, "EXPS mismatch"

assert fl_dbl(f32(-0.0946) + f32(0.5)) >> 16 == kz, "KZ mismatch"
assert fl_dbl(f32(0.23)) >> 16 == ko, "KO mismatch"
print(f"KZ={kz} KO={ko} cross-checked")

# ---- emit ----
L = []
L.append("// @generated by tools/enc-oracle/extract_quant_tables.py "
         "— do not edit.")
L.append("// Source data: QPROBE tables block (quant_probe output), "
         "cross-checked")
L.append("// against cpp/libAACenc/src/aacEnc_rom.cpp source forms.")


def emit_i16(name, vals, doc):
    L.append(f"/// {doc}")
    L.append(f"pub(crate) const {name}: [i16; {len(vals)}] = [")
    for i in range(0, len(vals), 12):
        L.append("    " + ", ".join(str(v) for v in vals[i:i + 12]) + ",")
    L.append("];")


def emit_i32(name, vals, doc):
    L.append(f"/// {doc}")
    L.append(f"pub(crate) const {name}: [i32; {len(vals)}] = [")
    for i in range(0, len(vals), 8):
        L.append("    " + ", ".join(str(v) for v in vals[i:i + 8]) + ",")
    L.append("];")


def emit_u8(name, vals, doc):
    L.append(f"/// {doc}")
    L.append(f"pub(crate) const {name}: [[u8; 14]; 4] = [")
    for r in range(4):
        L.append("    [" + ", ".join(str(v) for v in vals[14 * r:14 * r + 14])
                 + "],")
    L.append("];")


def emit_i32_4x14(name, vals, doc):
    L.append(f"/// {doc}")
    L.append(f"pub(crate) const {name}: [[i32; 14]; 4] = [")
    for r in range(4):
        L.append("    [" + ", ".join(str(v) for v in vals[14 * r:14 * r + 14])
                 + "],")
    L.append("];")


emit_i16("QUANT_TABLE_Q", qtab_q,
         "2^(0.25*q)/2, q in 0..4 (quantize.cpp:707; 16-bit on 32x16).")
emit_i16("QUANT_TABLE_E", qtab_e,
         "2^(0.75*e)/8, e in 0..4 (quantize.cpp:713; 16-bit on 32x16).")
emit_i16("MTAB_3_4", mtab34,
         "(0.5..1.0)^0.75 mantissa table (512 entries; 16-bit on 32x16).")
emit_i32("MTAB_4_3", mtab43, "mantissa^4/3 table (512 Q31 entries).")
emit_i32_4x14("SPEC_EXP_MANT", expm,
              "exponent multiplier per (gain mod 4, specExp).")
emit_u8("SPEC_EXP_SHIFT", exps,
        "exponent shifter per (gain mod 4, specExp).")
L.append(f"/// Dead-zone addend for dZoneQuantEnable=0 (LC): "
         f"FL2FXCONST_DBL(-0.0946f+0.5f)>>16.")
L.append(f"pub(crate) const QUANT_K0: i32 = {kz};")
L.append(f"/// Dead-zone addend for dZoneQuantEnable=1 (ELD-only here): "
         f"FL2FXCONST_DBL(0.23f)>>16.")
L.append(f"pub(crate) const QUANT_K1: i32 = {ko};")

anchor = "----------------------------------------------------------------------------- */"
_parts = open(HEADER_SRC).read().split(anchor)
assert len(_parts) == 2, f"header anchor missing in {HEADER_SRC}"
header = _parts[0] + anchor + "\n"
with open(OUT, "w") as f:
    f.write(header)
    f.write("\n".join(L) + "\n")
print(f"wrote {OUT}")
