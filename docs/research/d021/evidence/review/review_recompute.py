import re, os, json

D = "/private/var/folders/*/T/opencode/v4.29-bitattr-20261020"
BIT = "/tmp/v429-bitattr"
LIVE = os.path.join(D, "v429-live.log")

CASES = [("onset-stereo", 24, "onset-stereo"), ("onset-44100", 22, "onset-stereo-44100")]

def parse_real(fixture, frame):
    d = {"ch": {}}
    path = os.path.join(BIT, f"{fixture}_f{frame}_bitattr.txt")
    for ln in open(path):
        t = ln.split()
        if not t: continue
        k = t[0]
        if k == "QC_OUT":
            d["qc"] = {a: int(b) for a, b in (x.split("=") for x in t[1:])}
        elif k == "EL0":
            d["el"] = {a: int(b) for a, b in (x.split("=") for x in t[1:])}
        elif k == "EL0_PE":
            d["pe"] = {a: int(b) for a, b in (x.split("=") for x in t[1:])}
        elif k == "LOOP":
            d["loop"] = {a: int(b) for a, b in (x.split("=") for x in t[1:])}
        elif k == "MSMASK":
            d["mask"] = list(map(int, t[1:]))
        elif k == "MSDIGEST":
            d["digest"] = int(t[1])
        elif re.match(r"CH[01]_SCF$", k):
            d["ch"].setdefault(int(k[2]), {})["scf"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_MAXV$", k):
            d["ch"][int(k[2])]["maxv"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_QUANT$", k):
            d["ch"][int(k[2])]["quant"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_SECT$", k):
            ch = int(k[2]); v = {a: int(b) for a, b in (x.split("=") for x in t[1:])}
            d["ch"][ch]["sect"] = v; d["ch"][ch]["secs"] = []
        elif re.match(r"CH[01]_SEC\d+$", k):
            ch = int(k[2]); v = {a: int(b) for a, b in (x.split("=") for x in t[1:])}
            d["ch"][ch]["secs"].append(v)
        elif re.match(r"CH[01]_TNS_W\d+_F\d+$", k):
            m = re.match(r"CH([01])_TNS_W(\d+)_F(\d+)$", k)
            ch, w, f = map(int, m.groups())
            v = {a: int(b) for a, b in (x.split("=") for x in t[1:5])}
            ci = t.index("coef")
            d["ch"][ch].setdefault("tns", {})[(w, f)] = (v["len"], v["order"], list(map(int, t[ci+1:])))
    return d

def parse_live(name, frame):
    out = {}
    grab = False
    for ln in open(LIVE):
        l = ln.strip()
        if l.startswith(f"V429CASE {name} f={frame}"):
            grab = True
            out["case"] = l
            continue
        if l.startswith("V429CASE "):
            grab = False
        if not grab or not l.startswith("V429"):
            continue
        out.setdefault(l.split()[0], []).append(l)
    return out

def ints_after(s, tag):
    i = s.index(tag) + len(tag)
    return [int(x) for x in re.findall(r"-?\d+", s[i:])]

for fixture, frame, name in CASES:
    R = parse_real(fixture, frame)
    L = parse_live(name, frame)
    print("=" * 70)
    print(fixture, "f", frame)
    qc = R["qc"]
    total = qc["static"] + qc["usedDyn"] + qc["totFill"] + qc["align"] + qc["elExt"] + qc["globalExt"]
    print(" real total formula:", total, "reported", qc["total"], "ADTS bytes", (total + 56) / 8)
    ch_dyn = [R["ch"][c]["sect"]["side"] + R["ch"][c]["sect"]["huff"] + R["ch"][c]["sect"]["scf"] + R["ch"][c]["sect"]["noise"] for c in (0, 1)]
    print(" real ch dyn:", ch_dyn, "sum", sum(ch_dyn), "qc usedDyn", qc["usedDyn"])
    ls = L["V429LIVE_FINAL"][0]
    print(" live final:", ls)
    lcase = L["case"][0]
    print(" live case:", lcase)
    # live channel dyn from S5 sect lines
    ld = []
    for c in (0, 1):
        sect = [x for x in L[f"V429LIVE_CH{c}_SECT"]][0]
        vals = {a: int(b) for a, b in (x.split("=") for x in sect.split()[1:])}
        ld.append(vals["side"] + vals["huff"] + vals["scf"] + vals["noise"])
    print(" live ch dyn:", ld, "sum", sum(ld))
    # SCF diffs
    for c in (0, 1):
        rscf = R["ch"][c]["scf"]
        lscf = ints_after([x for x in L[f"V429LIVE_CH{c}_S4"]][0], "scf=")
        diffs = [(b, rscf[b], lscf[b]) for b in range(min(len(rscf), len(lscf))) if rscf[b] != lscf[b]]
        print(f" ch{c} scf diffs ({len(diffs)}):", diffs)
    # TNS diffs
    for c in (0, 1):
        ro = {w: R["ch"][c]["tns"][(w, 0)][1] for w in range(8)}
        lo = ints_after([x for x in L[f"V429LIVE_CH{c}_TNS"]][0], "order=")
        lorders = [(lo[2*w], lo[2*w+1]) for w in range(8)]
        print(f" ch{c} TNS orders real", [ro[w] for w in range(8)], "live", [x[0] for x in lorders])
        for w in range(8):
            rc = R["ch"][c]["tns"][(w, 0)][2][:R["ch"][c]["tns"][(w, 0)][1]]
            lc = ints_after([x for x in L[f"V429LIVE_CH{c}_TNSCOEF"]][0], "coef=")
            # skip detailed coef parse
        for w in range(8):
            r = R["ch"][c]["tns"][(w, 0)]
            if r[1] != lorders[w][0]:
                print(f"   window {w}: real order {r[1]} coefs {r[2][:r[1]]} | live order {lorders[w][0]}")
    print()