#!/usr/bin/env python3
"""Extract M4-S7 Huffman code tables from the C probe into Rust source.

Usage: extract_bitenc_tables.py [--rom PATH] [--out PATH] <s7probe-txt>   (repo root)

Options (all CWD-relative defaults, preserved for repo-root runs):
  --rom PATH   FDK ROM source (default: cpp/libAACenc/src/aacEnc_rom.cpp)
  --out PATH   output Rust file (default: rust/src/aac_enc/bitenc_tables.rs)

Reads the S7TABLES blocks of <s7probe-txt> (produced by bitenc_probe,
which prints the exact integers the reference build uses) and writes
rust/src/aac_enc/bitenc_tables.rs. Run `cargo fmt` afterwards: like
extract_bitcount_tables.py, this script emits fixed chunking and relies
on rustfmt for final layout (the committed file is script output +
rustfmt).

Every value is cross-checked against an INDEPENDENT parse of the C++
source text (cpp/libAACenc/src/aacEnc_rom.cpp): all tables are plain
hex/decimal integer initializers, so source text is exact.
Any mismatch aborts: tables are data, never hand-transcribed.

Note `huff_ctab11` is [21][17]: the encoder only indexes rows/cols
0..16 (`t = min(|v|,16)` in bit_cnt.cpp:915-916); rows 17..20 are
carried verbatim and never indexed by the encode path.
"""
import re
import sys

PROBE = None
ROM = "cpp/libAACenc/src/aacEnc_rom.cpp"
OUT = "rust/src/aac_enc/bitenc_tables.rs"

_ARGS = sys.argv[1:]
while _ARGS and _ARGS[0].startswith("--"):
    _opt = _ARGS[0]
    if _opt in ("--rom", "--out"):
        if len(_ARGS) < 2:
            raise SystemExit(
                "usage: extract_bitenc_tables.py [--rom PATH] [--out PATH] "
                "<s7probe-txt>   (repo root)")
        if _opt == "--rom":
            ROM = _ARGS[1]
        else:
            OUT = _ARGS[1]
        _ARGS = _ARGS[2:]
    elif _opt.startswith("--rom="):
        ROM = _opt[len("--rom="):]
        _ARGS = _ARGS[1:]
    elif _opt.startswith("--out="):
        OUT = _opt[len("--out="):]
        _ARGS = _ARGS[1:]
    else:
        raise SystemExit(f"unknown flag {_opt}; usage: extract_bitenc_tables.py "
                         f"[--rom PATH] [--out PATH] <s7probe-txt>   (repo root)")
if len(_ARGS) != 1:
    raise SystemExit("usage: extract_bitenc_tables.py [--rom PATH] [--out PATH] "
                     "<s7probe-txt>   (repo root)")
PROBE = _ARGS[0]

txt = open(PROBE).read().splitlines()


def ints(tag):
    for ln in txt:
        if ln.startswith(tag + " ") or ln == tag:
            return [int(x) for x in ln.split()[1:]]
    raise AssertionError(f"block {tag} missing")


# (probe tag, ROM symbol, count)
spec = [
    ("CTAB1", "FDKaacEnc_huff_ctab1", 81),
    ("CTAB2", "FDKaacEnc_huff_ctab2", 81),
    ("CTAB3", "FDKaacEnc_huff_ctab3", 81),
    ("CTAB4", "FDKaacEnc_huff_ctab4", 81),
    ("CTAB5", "FDKaacEnc_huff_ctab5", 81),
    ("CTAB6", "FDKaacEnc_huff_ctab6", 81),
    ("CTAB7", "FDKaacEnc_huff_ctab7", 64),
    ("CTAB8", "FDKaacEnc_huff_ctab8", 64),
    ("CTAB9", "FDKaacEnc_huff_ctab9", 169),
    ("CTAB10", "FDKaacEnc_huff_ctab10", 169),
    ("CTAB11", "FDKaacEnc_huff_ctab11", 21 * 17),
    ("CTABSCF", "FDKaacEnc_huff_ctabscf", 121),
]
probe = {}
for tag, _, n in spec:
    probe[tag] = ints(tag)
    assert len(probe[tag]) == n, f"{tag}: got {len(probe[tag])}, want {n}"

# ---- independent parse from source text ----
rom = open(ROM).read()


def get_body(name):
    m = re.search(name + r"\s*(\[[^\]]*\]\s*)+=\s*\{([^;]*)\};", rom, re.S)
    assert m, f"{name} not found"
    return m.group(2)


def rom_ints(name):
    body = get_body(name)
    return [int(x, 0) for x in re.findall(r"0[xX][0-9a-fA-F]+|\d+", body)]


for tag, name, n in spec:
    vals = rom_ints(name)
    assert len(vals) == n, f"{name}: got {len(vals)}, want {n}"
    assert vals == probe[tag], f"{name}: probe/source mismatch"


def chunks(vals, per_row):
    return [vals[i:i + per_row] for i in range(0, len(vals), per_row)]


RUST = [
    ("CTAB1", "HUFF_CTAB1", 81, 9, "u16", "FDKaacEnc_huff_ctab1[3][3][3][3]",
     "book 1"),
    ("CTAB2", "HUFF_CTAB2", 81, 9, "u16", "FDKaacEnc_huff_ctab2[3][3][3][3]",
     "book 2"),
    ("CTAB3", "HUFF_CTAB3", 81, 9, "u16", "FDKaacEnc_huff_ctab3[3][3][3][3]",
     "book 3"),
    ("CTAB4", "HUFF_CTAB4", 81, 9, "u16", "FDKaacEnc_huff_ctab4[3][3][3][3]",
     "book 4"),
    ("CTAB5", "HUFF_CTAB5", 81, 9, "u16", "FDKaacEnc_huff_ctab5[9][9]",
     "book 5"),
    ("CTAB6", "HUFF_CTAB6", 81, 9, "u16", "FDKaacEnc_huff_ctab6[9][9]",
     "book 6"),
    ("CTAB7", "HUFF_CTAB7", 64, 8, "u16", "FDKaacEnc_huff_ctab7[8][8]",
     "book 7"),
    ("CTAB8", "HUFF_CTAB8", 64, 8, "u16", "FDKaacEnc_huff_ctab8[8][8]",
     "book 8"),
    ("CTAB9", "HUFF_CTAB9", 169, 13, "u16", "FDKaacEnc_huff_ctab9[13][13]",
     "book 9"),
    ("CTAB10", "HUFF_CTAB10", 169, 13, "u16",
     "FDKaacEnc_huff_ctab10[13][13]", "book 10"),
    ("CTAB11", "HUFF_CTAB11", 357, 17, "u16",
     "FDKaacEnc_huff_ctab11[21][17]",
     "book 11 (ESC base; encoder indexes rows/cols 0..16 only)"),
    ("CTABSCF", "HUFF_CTABSCF", 121, 11, "u32",
     "FDKaacEnc_huff_ctabscf[121]", "scalefactor DPCM, index delta+60"),
]

with open(OUT, "w") as f:
    f.write("""/* Ferrite: this file is part of a Third-Party Modified Version of the
Fraunhofer FDK AAC Codec Library for Android. Modifications and Rust
implementation copyright © 2025–2026 Jeroen Steenvoorde. Modification
notice date: 2026-10-26. See rust/NOTICE for the full license text. */
/* -----------------------------------------------------------------------------
Software License for The Fraunhofer FDK AAC Codec Library for Android

(c) Copyright 2026 Fraunhofer-Gesellschaft zur Foerderung der angewandten
Forschung e.V. All rights reserved.

Generated by tools/enc-oracle/extract_bitenc_tables.py from the
bitenc_probe output, cross-checked value-for-value against
cpp/libAACenc/src/aacEnc_rom.cpp. Do not hand-edit: tables are data.
----------------------------------------------------------------------------- */
//! M4-S7 Huffman code tables (`aacEnc_rom.h:129-140`), script-extracted.
//!
//! Length tables live in [`super::bitcount_tables`]; code tables live here.
//! Flattened row-major in C declaration order.
""")
    for tag, rname, n, per_row, typ, csym, what in RUST:
        assert len(probe[tag]) == n
        width = 4 if typ == "u16" else 8
        f.write(f"""
/// `{csym}` ({what}).
pub(crate) const {rname}: [{typ}; {n}] = [\n""")
        for row in chunks(probe[tag], per_row):
            f.write("    " + ", ".join(f"0x{v:0{width}x}" for v in row) + ",\n")
        f.write("];\n")
print(f"wrote {OUT}")
