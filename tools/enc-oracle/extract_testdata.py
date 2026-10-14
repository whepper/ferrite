#!/usr/bin/env python3
"""Extract committed Rust testdata from enc_oracle dump files.

Usage: extract_testdata.py <oracle-dir> <testdata-dir>

Writes (M2, unchanged format):
  seq_<name>.txt                  all frames: "f ch seq shape groups g0 g1 g2 g3"
  mdct_<name>_f<F>_ch<C>.txt      "E" line + 1024 int32 coefficients
  ring_<name>_f<F>_ch<C>.txt      2048 int16 transform-input samples
Writes (M3):
  cfg_<rate>_<pch>.txt            PSY_CONFIGURATION long+short dumps + TNSCFG
  psy_<name>_f<F>_ch<C>.txt       PSY line, MAXSCALE, per-window NRG/NRGLD/THR/
                                  SPREAD, TNSW/TNSG lines, NM1/NM1STATE,
                                  GROUPED or LONGOUT tail
"""
import os
import sys

ORACLE_DIR, TESTDATA_DIR = sys.argv[1], sys.argv[2]

# (oracle stem, mdct frames [(frame, ch)], ring frames [(frame, ch)],
#  psy frames [(frame, ch)])
# NOTE: stereo psy frames need BOTH channels (end-to-end test compares ch1
# and MS vectors too); ms files are derived from the ch0 entry of each frame.
PLAN = {
    "m2-onset-stereo-48k": ([(22, 0), (23, 0), (24, 0), (24, 1), (25, 0), (26, 0)],
                             [(0, 0), (1, 0), (23, 0), (24, 0), (25, 0)],
                             [(22, 0), (22, 1), (23, 0), (23, 1), (24, 0), (24, 1),
                              (25, 0), (25, 1), (26, 0), (26, 1)]),
    "m2-stepdown-stereo-48k": ([(0, 0), (1, 0), (2, 0)], [],
                               [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (2, 1),
                                (5, 0), (5, 1)]),
    "m2-click-mono-48k": ([(11, 0)], [], [(11, 0)]),
    "m2-edge-mono-48k": ([], [], [(3, 0), (4, 0), (5, 0)]),
    "m2-sine-stereo-48k": ([(5, 0)], [], [(5, 0), (5, 1), (6, 0), (6, 1)]),
    "m2-impulse-mono-48k": ([(4, 0), (5, 0)], [], [(4, 0), (5, 0)]),
    "m2-silence-stereo-48k": ([(5, 0)], [], [(5, 0), (5, 1)]),
    "m3-multitone-stereo-48k": ([], [(0, 0)], [(1, 0), (1, 1), (5, 0), (5, 1)]),
    "m3-sweep-stereo-48k": ([(10, 0), (10, 1)], [(0, 0)], [(10, 0), (10, 1)]),
    "m3-noise-mono-48k": ([], [(0, 0)], [(5, 0)]),
    "m3-tonal-noise-stereo-48k": ([], [(0, 0)], [(5, 0), (5, 1)]),
    "m3-quiet-sine-stereo-48k": ([], [(0, 0)], [(5, 0), (5, 1)]),
    "m3-sine-stereo-441k": ([], [(0, 0)], [(5, 0), (5, 1)]),
    "m3-onset-stereo-441k": ([], [(21, 0), (22, 0)],
                             [(21, 0), (21, 1), (22, 0), (22, 1)]),
}

# (oracle stem for config snapshots; rate is taken from the dump)
CFG_SOURCES = [
    "m2-sine-stereo-48k",
    "m2-impulse-mono-48k",
    "m3-sine-stereo-441k",
]


def ints(rest):
    return [int(x) for x in rest.split()]


def parse(path):
    """Parse oracle dump. Returns (frames, cfg)."""
    raw = [ln for ln in open(path).read().splitlines() if ln.strip()]
    frames, cfg = {}, {}
    psy_blocks = []
    ms_runs = []
    cur_ms = None
    cur_cfg = None
    cur_frame_key = None
    i, n = 0, len(raw)
    while i < n:
        line = raw[i]
        if line.startswith("#"):
            i += 1
        elif line.startswith("CFG "):
            p = line.split()
            cur_cfg = (p[1], int(p[2].split("=")[1]))
            cfg[cur_cfg] = {"head": line.strip(), "extra": {}}
            i += 1
        elif line.split()[0] in ("OFFSETS", "PCMQ", "MASKHI", "MASKLO",
                                 "MASKHIS", "MASKLOS", "MINSNR"):
            tag, _, rest = line.partition(" ")
            cfg[cur_cfg]["extra"][tag] = ints(rest)
            i += 1
        elif line.startswith("TNSCFG "):
            p = line.split()
            cfg[(p[1], "tns")] = line.strip()
            i += 1
        elif line.startswith("FRAME "):
            p = line.split()
            key = (int(p[1]), int(p[3]))
            fr = {"meta": line.strip()}
            assert raw[i + 1].startswith("RING ")
            fr["ring"] = ints(raw[i + 1].split(" ", 1)[1])
            assert len(fr["ring"]) == 2048
            assert raw[i + 2].startswith("MDCT_E ")
            fr["e"] = int(raw[i + 2].split()[1])
            mdct_line = raw[i + 3]
            assert mdct_line == "MDCT" or mdct_line.startswith("MDCT "), \
                mdct_line[:30]
            fr["mdct"] = ints(mdct_line.split(" ", 1)[1] if " " in mdct_line
                              else "")
            assert len(fr["mdct"]) == 1024, len(fr["mdct"])
            i += 4
            frames[key] = fr
            cur_frame_key = key
        elif line.startswith("RXX1 "):
            # Ground-truth autocorrelation pairs, dumped per channel after
            # the frame's FRAME blocks (element-global rescale restructure):
            # attach to the most recent frame by ch tag.
            p1 = line.split()
            rch = int(p1[1].split("=")[1])
            w = int(p1[2].split("=")[1])
            rxx1 = [int(x) for x in p1[3:]]
            assert len(rxx1) == 13, len(rxx1)
            assert raw[i + 1].startswith("RXX2 ")
            p2 = raw[i + 1].split()
            assert p2[1] == f"ch={rch}" and p2[2] == f"w={w}", raw[i + 1][:40]
            rxx2 = [int(x) for x in p2[3:]]
            assert len(rxx2) == 13, len(rxx2)
            frames[(cur_frame_key[0], rch)].setdefault("rxx", []).append(
                (w, rxx1, rxx2))
            i += 2
        elif line[:3] in ("MSM", "MSS"):
            # Element-level MS block (stereo only): quadruplets per window
            # (MSMw, MSSw, MSMLDw, MSSLDw). Runs attach to stereo frames
            # positionally below.
            if cur_ms is None:
                cur_ms = []
            tag = line.split()[0]
            cur_ms.append((tag, ints(line.split(" ", 1)[1])))
            i += 1
            if len(cur_ms) % 4 == 0:
                nxt = raw[i][:3] if i < n else ""
                if nxt not in ("MSM", "MSS"):
                    ms_runs.append(cur_ms)
                    cur_ms = None
        elif line.startswith("PSY "):
            fr = {"psy": {k: int(v) for kv in line.split()[1:]
                          for k, v in [kv.split("=")]}}
            i += 1
            assert raw[i].startswith("MAXSCALE ")
            fr["maxscale"] = ints(raw[i].split(" ", 1)[1])
            i += 1
            for store in ("nrg", "nrgld", "thr", "spread"):
                fr[store] = {}
            while True:
                parts = raw[i].split(" ", 1)
                tag = parts[0]
                base = tag.rstrip("0123456789")
                if base in ("NRG", "NRGLD", "THR", "SPREAD"):
                    store = {"NRG": "nrg", "NRGLD": "nrgld", "THR": "thr",
                             "SPREAD": "spread"}[base]
                    w = int(tag[len(base):]) if len(tag) > len(base) else -1
                    fr[store][w] = ints(parts[1] if len(parts) > 1 else "")
                    i += 1
                else:
                    break
            fr["tnsw"], fr["tnsg"] = [], []
            while raw[i].startswith("TNSW "):
                fr["tnsw"].append(raw[i][len("TNSW "):])
                assert raw[i + 1].startswith("TNSG ")
                fr["tnsg"].append(raw[i + 1][len("TNSG "):])
                i += 2
            assert raw[i].startswith("NM1 ")
            fr["nm1"] = ints(raw[i].split(" ", 1)[1])
            assert raw[i + 1].startswith("NM1STATE ")
            fr["nm1state"] = raw[i + 1].split()[1:]
            i += 2
            if raw[i].startswith("GROUPED "):
                fr["grouped"] = {"head": raw[i]}
                i += 1
                while True:
                    tag = raw[i].split()[0]
                    assert tag in ("GE", "GT", "GS", "GELD", "GTLD", "GOFF",
                                   "GMSNR", "GSPEC"), tag
                    fr["grouped"][tag] = ints(raw[i].split(" ", 1)[1])
                    i += 1
                    if tag == "GSPEC":
                        break
            elif raw[i].startswith("LONGOUT "):
                fr["longout"] = {"head": raw[i]}
                assert raw[i + 1].startswith("TLD ")
                fr["longout"]["TLD"] = ints(raw[i + 1].split(" ", 1)[1])
                i += 2
            else:
                raise AssertionError(f"unexpected tail {raw[i][:30]}")
            psy_blocks.append(fr)
        else:
            raise AssertionError(f"unexpected line {line[:40]}")
    # PSY blocks follow FRAME blocks in (frame, ch) order; associate
    # positionally (per-frame output is grouped by stage, not channel).
    # MS runs likewise attach to stereo frames in file order.
    order = sorted(frames)
    assert len(psy_blocks) == len(order), (len(psy_blocks), len(order))
    for key, psy in zip(order, psy_blocks):
        frames[key].update(psy)
    # MS runs exist only for stereo files, one per frame.
    if ms_runs:
        assert cur_ms is None
        nch = max(ch for (_, ch) in order) + 1
        assert nch == 2, nch
        assert len(ms_runs) == len(order) // 2, (len(ms_runs), len(order))
        for idx, f in enumerate(sorted({fr for (fr, _) in order})):
            frames[(f, 0)]["ms"] = ms_runs[idx]
            frames[(f, 1)]["ms"] = ms_runs[idx]
    return frames, cfg


def short_name(stem):
    return stem.replace("m2-", "").replace("m3-", "").replace("-48k", "") \
               .replace("-441k", "-44100")


def main():
    os.makedirs(TESTDATA_DIR, exist_ok=True)
    for stem, (mdct_frames, ring_frames, psy_frames) in PLAN.items():
        frames, _ = parse(os.path.join(ORACLE_DIR, stem + ".oracle.txt"))
        name = short_name(stem)
        with open(os.path.join(TESTDATA_DIR, f"seq_{name}.txt"), "w") as f:
            for (fr, ch) in sorted(frames):
                p = frames[(fr, ch)]["meta"].split()
                f.write(f"{p[1]} {p[3]} {p[5]} {p[7]} {p[9]} "
                        f"{p[10]} {p[11]} {p[12]} {p[13]}\n")
        for fr, ch in mdct_frames:
            d = frames[(fr, ch)]
            with open(os.path.join(TESTDATA_DIR,
                                   f"mdct_{name}_f{fr}_ch{ch}.txt"), "w") as f:
                f.write(f"{d['e']}\n")
                f.write(" ".join(map(str, d["mdct"])) + "\n")
        for fr, ch in ring_frames:
            d = frames[(fr, ch)]
            with open(os.path.join(TESTDATA_DIR,
                                   f"ring_{name}_f{fr}_ch{ch}.txt"), "w") as f:
                f.write(" ".join(map(str, d["ring"])) + "\n")
        for fr, ch in psy_frames:
            d = frames[(fr, ch)]
            with open(os.path.join(TESTDATA_DIR,
                                   f"psy_{name}_f{fr}_ch{ch}.txt"), "w") as f:
                p = d["psy"]
                f.write(f"mdctScale={p['mdctScale']} "
                        f"finalShift={p['finalShift']} "
                        f"tnsShift={p['tnsShift']} "
                        f"nrgScaling={p['nrgScaling']}\n")
                f.write("MAXSCALE " + " ".join(map(str, d["maxscale"])) + "\n")
                for store, tag in (("nrg", "NRG"), ("nrgld", "NRGLD"),
                                   ("thr", "THR"), ("spread", "SPREAD")):
                    for w in sorted(d[store]):
                        suffix = "" if w == -1 else str(w)
                        f.write(f"{tag}{suffix} " +
                                " ".join(map(str, d[store][w])) + "\n")
                for tnsw, tnsg in zip(d["tnsw"], d["tnsg"]):
                    f.write(f"TNSW {tnsw}\nTNSG {tnsg}\n")
                f.write("NM1 " + " ".join(map(str, d["nm1"])) + "\n")
                f.write("NM1STATE " + " ".join(d["nm1state"]) + "\n")
                if "grouped" in d:
                    f.write(d["grouped"]["head"] + "\n")
                    for tag in ("GE", "GT", "GS", "GELD", "GTLD", "GOFF",
                                "GMSNR", "GSPEC"):
                        f.write(f"{tag} " +
                                " ".join(map(str, d["grouped"][tag])) + "\n")
                if "longout" in d:
                    f.write(d["longout"]["head"] + "\n")
                    f.write("TLD " + " ".join(map(str, d["longout"]["TLD"]))
                            + "\n")
                with open(os.path.join(TESTDATA_DIR,
                                       f"rxx_{name}_f{fr}_ch{ch}.txt"), "w") as f:
                    for w, rxx1, rxx2 in d.get("rxx", []):
                        f.write(f"RXX1 w={w} " + " ".join(map(str, rxx1)) + "\n")
                        f.write(f"RXX2 w={w} " + " ".join(map(str, rxx2)) + "\n")
            if "ms" in d and ch == 0:
                # Element-level MS vectors, one file per frame.
                with open(os.path.join(TESTDATA_DIR,
                                       f"ms_{name}_f{fr}.txt"), "w") as f:
                    for tag, vals in d["ms"]:
                        f.write(f"{tag} " + " ".join(map(str, vals)) + "\n")
        print(f"{stem}: {len(frames)} frames extracted")
    for stem in CFG_SOURCES:
        _, cfg = parse(os.path.join(ORACLE_DIR, stem + ".oracle.txt"))
        rates = sorted({sr for (_, sr) in cfg if isinstance(sr, int)})
        assert len(rates) == 1, rates
        # per-channel bitrate from the header comment of the dump
        head = open(os.path.join(ORACLE_DIR, stem + ".oracle.txt")).readline()
        pch = int(head.split("chBitrate=")[1])
        name = f"cfg_{rates[0]}_{pch}.txt"
        with open(os.path.join(TESTDATA_DIR, name), "w") as f:
            for kind in ("LONG", "SHORT"):
                c = cfg[(kind, rates[0])]
                f.write(c["head"] + "\n")
                for tag in ("OFFSETS", "PCMQ", "MASKHI", "MASKLO", "MASKHIS",
                            "MASKLOS", "MINSNR"):
                    f.write(f"{tag} " + " ".join(map(str, c["extra"][tag]))
                            + "\n")
                f.write(cfg[(kind, "tns")] + "\n")
        print(f"{stem}: config {name} written")


main()
