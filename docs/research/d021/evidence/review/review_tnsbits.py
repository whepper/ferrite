import re, os
D = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/v4.29-bitattr-20261020"
BIT = "/tmp/v429-bitattr"
def real_vec(path, tag):
    for ln in open(path):
        if ln.startswith(tag + " "):
            return [int(x) for x in ln.split()[1:]]
def live_vec(log, name, frame, tag):
    grab = False
    for ln in open(log):
        l = ln.strip()
        if l.startswith(f"V429CASE {name} f={frame}"): grab = True; continue
        if l.startswith("V429CASE "): grab = False
        if grab and l.startswith("V429LIVE_" + tag + " "):
            return [int(x) for x in re.findall(r"-?\d+", l.split(" ", 1)[1])]
for fixture, frame, name in [("onset-stereo", 24, "onset-stereo"), ("onset-44100", 22, "onset-stereo-44100")]:
    p = os.path.join(BIT, f"{fixture}_f{frame}_bitattr.txt")
    for c in (0, 1):
        rm = real_vec(p, f"CH{c}_MAXV")
        lm = live_vec(os.path.join(D, "v429-live.log"), name, frame, f"CH{c}_MAXV")
        d = [(b, rm[b], lm[b]) for b in range(56) if rm[b] != lm[b]]
        print(fixture, f"ch{c} MAXV diffs", d)
    # TNS bit counts per channel from captured data using C formula
    for c in (0, 1):
        tns = {}
        for ln in open(p):
            m = re.match(rf"CH{c}_TNS_W(\d+)_F(\d+) ", ln)
            if m:
                t = ln.split()
                v = {a: int(b) for a, b in (x.split("=") for x in t[1:5])}
                ci = t.index("coef")
                tns[(int(m.group(1)), int(m.group(2)))] = (v["len"], v["order"], list(map(int, t[ci+1:])))
        # formula: per window 1 present bit; if numFilters: 1 + 4 len + 3 order + dir + cc + order*coefBits
        num = [0]*8
        for (w, f), (length, order, coefs) in tns.items():
            if f == 0 and order > 0: num[w] = 1
        bits = 0
        for w in range(8):
            bits += 1
            if num[w]:
                length, order, coefs = tns[(w, 0)]
                coef_bits = 3 if any(x < -2 or x > 1 for x in coefs[:order]) else 2
                bits += 1 + 4 + 3 + 1 + 1 + order * coef_bits
        print(fixture, f"ch{c} TNS data bits", bits, "+present =", bits + 1)