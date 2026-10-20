#!/usr/bin/env python3
"""V4.29 real-C vs live-Rust bit attribution analysis (onset f24, onset441 f22)."""
import re, sys

BITATTR = "/tmp/v429-bitattr"
LIVELOG = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.29-bitattr-20261020/v429-live.log"

def parse_real(fixture, frame):
    d = {}
    for l in open(f"{BITATTR}/{fixture}_f{frame}_bitattr.txt"):
        t = l.split()
        k = t[0]
        if k == "QC_OUT": d["qcout"] = dict(zip(
            ["grantedDyn","maxDyn","totFill","elExt","globalExt","static","usedDyn","align","total","globHdr","bitResTot"],
            [int(x.split("=")[1]) for x in t[1:]]))
        elif k == "EL0": d["el0"] = dict(zip(
            ["staticUsed","dynUsed","extUsed","grantedDyn","grantedPe","grantedPeCorr"],
            [int(x.split("=")[1]) for x in t[1:]]))
        elif k == "EL0_PE": d["pe"] = dict(zip(
            ["pe","constPart","nActive","offset","ch0pe","ch1pe"],
            [int(x.split("=")[1]) for x in t[1:]]))
        elif k == "LOOP": d["loop"] = dict(zip(["iterations","crashed"], [int(x.split("=")[1]) for x in t[1:]]))
        elif k == "MSDIGEST": d["digest"] = int(t[1])
        elif k == "MSMASK": d["mask"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]$", k):
            ch = int(k[2]); vals = dict(x.split("=") for x in t[1:])
            d.setdefault("ch", {})[ch] = {"gain": int(vals["gain"]), "nSfb": int(vals["nSfb"])}
        elif re.match(r"CH[01]_SCF$", k): d["ch"][int(k[2])]["scf"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_MAXV$", k): d["ch"][int(k[2])]["maxv"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_QUANT$", k): d["ch"][int(k[2])]["quant"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_SECT$", k):
            ch = int(k[2]); vals = dict(x.split("=") for x in t[1:])
            d["ch"][ch]["sect"] = {kk: int(vals[kk]) for kk in ["n","side","huff","scf","noise","firstScf"]}
            d["ch"][ch]["secs"] = []
        elif re.match(r"CH[01]_SEC\d+$", k):
            ch = int(k[2]); vals = dict(x.split("=") for x in t[1:])
            d["ch"][ch]["secs"].append(tuple(int(vals[kk]) for kk in ["book","start","cnt","bits"]))
        elif re.match(r"CH[01]_TNS_NUMFILT$", k): d["ch"][int(k[2])]["tns_num"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_TNS_COEFRES$", k): d["ch"][int(k[2])]["tns_res"] = list(map(int, t[1:]))
        elif re.match(r"CH[01]_TNS_W\d+_F\d+$", k):
            m = re.match(r"CH([01])_TNS_W(\d+)_F(\d+)$", k)
            ch, w, f = map(int, m.groups())
            vals = dict(x.split("=") for x in t[1:5])
            coefs = list(map(int, t[5][5:] and t[5:] or [])) if False else None
            # format: len=.. order=.. dir=.. cc=.. coef <12 ints>
            ci = t.index("coef")
            coefs = list(map(int, t[ci+1:]))
            d["ch"][ch].setdefault("tns", []).append((w, f, int(vals["len"]), int(vals["order"]), int(vals["dir"]), int(vals["cc"]), coefs))
    return d

def tns_data_bits(num, res, filts):
    """Exact C bitenc.cpp:536-573 count for SHORT (8 windows). filts: dict (w,f)->(len,order,dir,cc,coefs)."""
    present = any(n != 0 for n in num)
    if not present: return 0
    bits = 0
    for w in range(8):
        bits += 1
        nf = num[w]
        if nf == 0: continue
        bits += 1
        for f in range(nf):
            ln, order, dr, cc, coefs = filts[(w, f)]
            bits += 4 + 3
            if order != 0:
                bits += 1 + 1
                if res[w] == 4:
                    cb = 3
                    for k in range(order):
                        if not (-4 <= coefs[k] <= 3): cb = 4; break
                else:
                    cb = 2
                    for k in range(order):
                        if not (-2 <= coefs[k] <= 1): cb = 3; break
                bits += order * cb
    return bits

def live_parse():
    cases = {}
    cur = None
    for l in open(LIVELOG):
        l = l.strip()
        m = re.match(r"V429CASE (\S+) f=(\d+) total=(\d+) static=(\d+) pe=(\d+) dyn=(\d+) gains=(\[.*?\]) fill=(\d+) align=(\d+)", l)
        if m:
            cur = (m.group(1), int(m.group(2)))
            cases[cur] = {"case": [int(x) for x in [m.group(3), m.group(4), m.group(5), m.group(6), m.group(8), m.group(9)]],
                          "gains": m.group(7), "lines": []}
            continue
        if l.startswith("V429LIVE") and cur:
            cases[cur]["lines"].append(l)
    return cases

def live_get(lines, prefix):
    return [l for l in lines if l.startswith(prefix)]

def parse_vecn(s):
    return list(map(int, re.search(r"\[(.*)\]", s).group(1).split(",")))

for fixture, frame, real_len, live_len in [("onset-stereo", 24, 309, 337), ("onset-44100", 22, 303, 310)]:
    R = parse_real(fixture, frame)
    print(f"===== {fixture} f{frame} =====")
    q = R["qcout"]
    print(f"REAL frame: static={q['static']} used={q['usedDyn']} fill={q['totFill']} align={q['align']} elExt={q['elExt']} globExt={q['globalExt']} total={q['total']} globHdr={q['globHdr']}")
    print(f"  recon: {q['static']}+{q['usedDyn']}+{q['totFill']}+{q['align']}+{q['elExt']}+{q['globalExt']}={q['static']+q['usedDyn']+q['totFill']+q['align']+q['elExt']+q['globalExt']} vs total {q['total']}; total+hdr={(q['total']+q['globHdr'])} vs {real_len*8} ({real_len}B)")
    print(f"  grants: grantedDyn={q['grantedDyn']} grantedPe={R['el0']['grantedPe']} bitResLevelEl=0 bitResTot={q['bitResTot']} PE={R['pe']['pe']} const={R['pe']['constPart']} nAct={R['pe']['nActive']}")
    print(f"  loop: iter={R['loop']['iterations']} crashed={R['loop']['crashed']} gains=[{R['ch'][0]['gain']},{R['ch'][1]['gain']}]")
    for ch in [0, 1]:
        s = R["ch"][ch]["sect"]
        print(f"  REAL ch{ch} sect: n={s['n']} side={s['side']} huff={s['huff']} scf={s['scf']} noise={s['noise']} first={s['firstScf']} sum={s['side']+s['huff']+s['scf']+s['noise']}")
    # TNS bit math from real decisions
    for ch in [0, 1]:
        c = R["ch"][ch]
        filts = {(w, f): (ln, od, dr, cc, co) for (w, f, ln, od, dr, cc, co) in c["tns"]}
        tb = tns_data_bits(c["tns_num"], c["tns_res"], filts)
        print(f"  REAL ch{ch} TNS data bits (computed) = {tb} static-contrib = {1+tb}")

print()
LC = live_parse()
for (name, f), c in LC.items():
    tot, st, pe, dyn, fill, align = c["case"]
    print(f"----- live {name} f{f}: total={tot} static={st} pe={pe} dyn={dyn} gains={c['gains']} fill={fill} align={align}")
    print(f"  recon: {st}+{dyn}+{fill}+{align}+0+3={st+dyn+fill+align+3} vs {tot}; total+56={tot+56} vs ADTS bytes {tot+56}//8={(tot+56)//8}")
    for l in live_get(c["lines"], "V429LIVE grantedDyn"):
        print(f"  {l}")
    for l in live_get(c["lines"], "V429LIVE_FINAL"):
        print(f"  {l}")
    for l in live_get(c["lines"], "V429LIVE_PASS"):
        print(f"  {l}")
    for l in live_get(c["lines"], "V429LIVE_STATIC"):
        print(f"  {l}")
    for l in c["lines"]:
        if re.match(r"V429LIVE_CH[01]_SECT", l): print(f"  {l}")
    for l in c["lines"]:
        if re.match(r"V429LIVE_CH[01]_S4", l): print(f"  {l[:200]}")
