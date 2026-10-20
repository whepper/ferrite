import re, os
D = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.29-bitattr-20261020"
BIT = "/tmp/v429-bitattr"

def real_vec(path, tag):
    for ln in open(path):
        if ln.startswith(tag + " "):
            return [int(x) for x in ln.split()[1:]]
    return None

def live_vec(log, name, frame, tag):
    grab = False
    for ln in open(log):
        l = ln.strip()
        if l.startswith(f"V429CASE {name} f={frame}"): grab = True; continue
        if l.startswith("V429CASE "): grab = False
        if grab and l.startswith("V429LIVE_" + tag + " "):
            return [int(x) for x in re.findall(r"-?\d+", l.split(" ", 1)[1])]
    return None

for fixture, frame, name in [("onset-stereo", 24, "onset-stereo"), ("onset-44100", 22, "onset-stereo-44100")]:
    p = os.path.join(BIT, f"{fixture}_f{frame}_bitattr.txt")
    rq = real_vec(p, "CH0_QUANT")
    lq = live_vec(os.path.join(D, "v429-live.log"), name, frame, "CH0_QFULL")
    print("=" * 60, fixture)
    # band ranges from real ms_pre offsets
    offs = None
    for ln in open(f"/tmp/v425-out-instr/{fixture}/{fixture}_f{frame}_ch0_ms_pre.txt"):
        if ln.startswith("OFFSETS"):
            offs = [int(x) for x in ln.split()[1:]]; break
    for b in range(56):
        if b in (13, 27, 41, 55): continue
        s, e = offs[b], offs[b+1]
        rs, ls = rq[s:e], lq[s:e]
        if any(rs) or any(ls):
            rnz = sum(1 for x in rs if x); lnz = sum(1 for x in ls if x)
            if rnz != lnz or b in (14,15,16,28,30,38,39,46,31,43,44,45,51,52,53,54):
                print(f" band {b:2d} [{s},{e}): real nz={rnz:3d} sumabs={sum(abs(x) for x in rs):7d} | live nz={lnz:3d} sumabs={sum(abs(x) for x in ls):7d}")
    # first diff line
    d = [i for i in range(1024) if rq[i] != lq[i]]
    print(" first quant diff:", d[0] if d else None, "count", len(d))