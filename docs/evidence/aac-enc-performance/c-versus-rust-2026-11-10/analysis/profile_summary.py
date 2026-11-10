#!/usr/bin/env python3
"""Summarize macOS `sample` top-of-stack profiles for C and Rust."""
import csv
import os
import re

T = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10"
OUT = os.path.join(T, "analysis")

FILES = {
    ("c", "music"): "sample-c-music.txt",
    ("r", "music"): "sample-r-music.txt",
    ("c", "speech"): "sample-c-speech.txt",
    ("r", "speech"): "sample-r-speech.txt",
}


def parse(path):
    lines = open(path).read().splitlines()
    root = None
    for i, line in enumerate(lines):
        if line.strip() == "Call graph:":
            for nxt in lines[i + 1:]:
                m = re.match(r"\s+(\d+)\s+Thread_", nxt)
                if m:
                    root = int(m.group(1))
                    break
            break
    start = None
    for i, line in enumerate(lines):
        if line.startswith("Sort by top of stack"):
            start = i + 1
            break
    top = []
    if start is not None:
        for line in lines[start:]:
            if not line.strip():
                break
            m = re.match(r"\s+(.+?)\s+\(in .*?\)\s+(\d+)\s*$", line)
            if not m:
                m = re.match(r"\s+(.+?)\s+(\d+)\s*$", line)
            if m:
                top.append((m.group(1).strip(), int(m.group(2))))
    return root, top


def stage_c(sym):
    s = sym.lower()
    if any(k in s for k in ["calcsfbdist", "estimatescalefactors", "quantizespectrum",
                            "calcsfbquantenergyanddist", "flog2", "initavoidholeflag"]):
        return "s4_scf_quant"
    if any(k in s for k in ["dynbitcount", "codevalues", "count1_2_3", "countesc",
                            "channelelementwrite", "encodespectral", "encodescalefactor"]):
        return "bit_path"
    if any(k in s for k in ["tnsdetect", "dit_fft", "dct_iv", "calcformfactor",
                            "blockswitching", "intensitystereo", "msstereoprocessing",
                            "calcbandnrgmsopt", "checkbandenergyoptim", "pecalculation"]):
        return "frontend_psy"
    if any(k in s for k in ["adjustthresholds", "qcmain", "calcformfactor"]):
        return "rate_control"
    if any(k in s for k in ["memmove", "memset", "malloc", "free", "deinterleave"]):
        return "memory_feeder"
    if any(k in s for k in ["encode_once", "aacencencode"]):
        return "driver_api"
    return "other"


def stage_r(sym):
    s = sym
    if any(k in s for k in ["calc_sfb_dist", "estimate_scale_factors", "quantize_spectrum",
                            "calc_sfb_quant_energy_and_dist", "calc_ld_data",
                            "calc_spec_pe_diff", "init_avoid_hole"]):
        return "s4_scf_quant"
    if any(k in s for k in ["dyn_bit_count", "encode_spectral_data", "encode_scale_factor_data",
                            "bit_count", "count_esc", "code_values", "count_scf_bits_diff"]):
        return "bit_path"
    if any(k in s for k in ["autocorr_quarter_lags", "block_switch", "form_factor",
                            "intensity_stereo", "dit_fft", "process_pair_reuse", "dctiv",
                            "energy", "spreading_max", "tns", "ms_stereo"]):
        return "frontend_psy"
    if any(k in s for k in ["adapt_thresholds_vbr", "calc_max_value_in_sfb", "qc_main_frame"]):
        return "rate_control"
    if any(k in s for k in ["memmove", "memset", "malloc", "free", "push_interleaved", "PcmFeeder"]):
        return "memory_feeder"
    if any(k in s for k in ["Encoder4push", "step_frame", "encode_once", "mach_absolute_time"]):
        return "driver_api"
    return "other"


symbol_rows = []
stage_rows = []
for (impl, workload), fname in FILES.items():
    root, top = parse(os.path.join(T, "logs", fname))
    total_top = sum(n for _, n in top)
    for sym, n in top:
        stage = stage_c(sym) if impl == "c" else stage_r(sym)
        symbol_rows.append({
            "impl": impl, "workload": workload, "symbol": sym,
            "self_samples": n,
            "pct_root": n / root * 100,
            "pct_top": n / total_top * 100 if total_top else 0,
            "stage": stage,
        })
    stages = {}
    for sym, n in top:
        stage = stage_c(sym) if impl == "c" else stage_r(sym)
        stages[stage] = stages.get(stage, 0) + n
    for stage, n in sorted(stages.items(), key=lambda kv: -kv[1]):
        stage_rows.append({
            "impl": impl, "workload": workload, "stage": stage,
            "self_samples": n,
            "pct_root": n / root * 100,
            "pct_top": n / total_top * 100 if total_top else 0,
        })

with open(os.path.join(OUT, "profile-symbols.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(symbol_rows[0]), delimiter="\t")
    w.writeheader()
    for r in symbol_rows:
        w.writerow({k: (f"{v:.2f}" if isinstance(v, float) else v) for k, v in r.items()})

with open(os.path.join(OUT, "profile-stages.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(stage_rows[0]), delimiter="\t")
    w.writeheader()
    for r in stage_rows:
        w.writerow({k: (f"{v:.2f}" if isinstance(v, float) else v) for k, v in r.items()})

for (impl, workload), fname in FILES.items():
    root, top = parse(os.path.join(T, "logs", fname))
    print(f"=== {impl} {workload} root={root} top={sum(n for _, n in top)}")
    for r in stage_rows:
        if r["impl"] == impl and r["workload"] == workload:
            print(f"  {r['stage']:<18} {r['pct_root']:6.2f}% root  {r['pct_top']:6.2f}% top")
