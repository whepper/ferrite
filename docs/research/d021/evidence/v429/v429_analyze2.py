#!/usr/bin/env python3
"""V4.29 part 2: S4->S5 causality (scf/qmax/quant/sections per band)."""
import re

BITATTR = "/tmp/v429-bitattr"
LIVELOG = "/private/var/folders/*/T/opencode/v4.29-bitattr-20261020/v429-live.log"

def parse_real(fixture, frame):
    d = {"ch": {}}
    for l in open(f"{BITATTR}/{fixture}_f{frame}_bitattr.txt"):
        t = l.split(); k = t[0]
        if k == "MSMASK": d["mask"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_SCF$", k): d["ch"].setdefault(int(k[2]), {})["scf"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_MAXV$", k): d["ch"][int(k[2])]["maxv"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_QUANT$", k): d["ch"][int(k[2])]["quant"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_SECT$", k):
            ch = int(k[2]); vals = dict(x.split("=") for x in t[1:])
            d["ch"][ch]["sect"] = {kk: int(vals[kk]) for kk in ["n","side","huff","scf","noise","firstScf"]}
            d["ch"][ch]["secs"] = []
        elif re.match(r"CH[01]_SEC\d+$", k):
            ch = int(k[2]); vals = dict(x.split("=") for x in t[1:])
            d["ch"][ch]["secs"].append(tuple(int(vals[kk]) for kk in ["book","start","cnt","bits"]))
        elif re.match(r"CH[01]_TNS_W\d+_F\d+$", k):
            m = re.match(r"CH([01])_TNS_W(\d+)_F(\d+)$", k)
            ch, w, f = map(int, m.groups())
            vals = dict(x.split("=") for x in t[1:5])
            ci = t.index("coef")
            d["ch"][ch].setdefault("tns", {})[(w, f)] = (int(vals["len"]), int(vals["order"]), list(map(int, t[ci+1:])))
        elif re.match(r"CH[01]_TNS_NUMFILT$", k): d["ch"][int(k[2])]["tns_num"] = list(map(int, t[1:]))
    return d

def live_block(name, frame):
    out = {}
    grab = False
    for l in open(LIVELOG):
        l = l.strip()
        if l.startswith(f"V429CASE {name} f={frame}"): grab = True; continue
        if l.startswith("V429CASE "): grab = False
        if grab and l.startswith("V429LIVE"):
            for p in ["CH0_S4", "CH1_S4", "CH0_MAXV", "CH1_MAXV", "CH0_QFULL", "CH1_QFULL",
                      "CH0_SECT", "CH1_SECT", "CH0_TNS ", "CH1_TNS ", "CH0_TNSCOEF", "CH1_TNSCOEF",
                      "CH0_SEC", "CH1_SEC"]:
                if l.startswith("V429LIVE_" + p):
                    out.setdefault(p.strip(), []).append(l)
    return out

def getvec(s):
    return list(map(int, re.search(r"\[(.*)\]", s).group(1).split(",")))

def band_of_line(soff, idx):
    for b in range(len(soff) - 1):
        if soff[b] <= idx < soff[b+1]: return b
    return -1

for fixture, name, frame in [("onset-stereo", "onset-stereo", 24), ("onset-44100", "onset-stereo-44100", 22)]:
    R = parse_real(fixture, frame)
    L = live_block(name, frame)
    print(f"===== {fixture} f{frame}: S4->S5 =====")
    # offsets: grouped short offsets needed for band mapping; derive from V4.25 ms_pre? use fixed 48k short grouped offsets
    # Instead compare per-band vectors directly (56 bands both sides, same geometry).
    for ch in [0]:
        rscf = R["ch"][ch]["scf"]; lscf = getvec(L[f"CH{ch}_S4"][0].split("scf=")[1])
        diffs = [(b, a, c) for b, (a, c) in enumerate(zip(rscf, lscf)) if a != c]
        print(f"ch{ch} SCF diffs (real vs live) bands: {diffs}")
        rmax = R["ch"][ch]["maxv"]; lmax = getvec(L[f"CH{ch}_MAXV"][0].split(" ", 1)[1])
        mdiffs = [(b, a, c) for b, (a, c) in enumerate(zip(rmax, lmax)) if a != c]
        print(f"ch{ch} QMAX diffs (real vs live): {mdiffs[:20]}{'...' if len(mdiffs)>20 else ''} (n={len(mdiffs)})")
        rq = R["ch"][ch]["quant"]; lq = getvec(L[f"CH{ch}_QFULL"][0].split(" ", 1)[1])
        qd = [(i, a, c) for i, (a, c) in enumerate(zip(rq, lq)) if a != c]
        print(f"ch{ch} QUANT diff lines: n={len(qd)} first={(qd[0] if qd else None)}")
        # per-band nz/sumabs comparison
        import itertools
        # need offsets: reconstruct from real ms_pre file
        soff = None
        for l in open(f"/tmp/v425-out-instr/{fixture}/{fixture}_f{frame}_ch0_ms_pre.txt"):
            if l.startswith("OFFSETS"): soff = list(map(int, l.split()[1:])); break
        for b in sorted(set(band_of_line(soff, i) for i, _, _ in qd))[:14]:
            s, e = soff[b], soff[b+1]
            rs = rq[s:e]; ls = lq[s:e]
            print(f"  band{b} [{s},{e}): real nz={sum(1 for x in rs if x)} sumabs={sum(abs(x) for x in rs)} scf={rscf[b]} | live nz={sum(1 for x in ls if x)} sumabs={sum(abs(x) for x in ls)} scf={lscf[b]}")
    # sections
    print("REAL secs ch0:", [(b, s, c, bi) for (b, s, c, bi) in R["ch"][0]["secs"]])
    for l in L["CH0_SECT"]: print("LIVE", l)
    print("REAL secs ch1:", [(b, s, c, bi) for (b, s, c, bi) in R["ch"][1]["secs"]])
    for l in L["CH1_SECT"]: print("LIVE", l)
    # TNS decision diff
    print("TNS decision diffs (real vs live):")
    for ch in [0, 1]:
        rl = L[f"CH{ch}_TNS"][0]
        rnum = R["ch"][ch]["tns_num"]
        lnum = [int(x) for x in re.search(r"num=\[(.*?)\]", rl).group(1).split(",")]
        lorders = [(int(a), int(b)) for a, b in re.findall(r"\[(\d+), (\d+)\]", rl.split("order=")[1].split(" dir=")[0])]
        print(f"  ch{ch} real num={rnum}")
        print(f"  ch{ch} live num={lnum}")
        ro = {k: v[1] for k, v in R["ch"][ch]["tns"].items() if k[1] == 0}
        print(f"  ch{ch} real orders w0-7: {[ro.get((w,0),None) for w in range(8)]}")
        print(f"  ch{ch} live orders w0-7: {[lorders[w][0] if w < len(lorders) else None for w in range(8)]}")
    # live per-section detail
    for ch in [0, 1]:
        det = [x for x in L.get(f"CH{ch}_SEC", []) if re.match(rf"V429LIVE_CH{ch}_SEC\d+ ", x)]
        print(f"LIVE secs ch{ch}:", [" ".join(x.split()[1:]) for x in det])
    print()
