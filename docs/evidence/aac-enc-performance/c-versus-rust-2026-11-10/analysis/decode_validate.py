#!/usr/bin/env python3
"""Decode all final C and Rust outputs with the reference aac-dec."""
import csv
import os
import struct
import subprocess

T = "/private/var/folders/6h/83xm7ljx4sz5njxdhkjsq53h0000gn/T/opencode/aac-c-vs-rust-2026-11-10"
rows = list(csv.DictReader(open(os.path.join(T, "analysis/per-workload.tsv")), delimiter="\t"))
out = []
for r in rows:
    stem = r["stem"]
    for impl, src in (("c", f"out/c-post-1/{stem}.c.aac"), ("r", f"out/r-post-1/{stem}.rust.aac")):
        dst = os.path.join(T, "decoded", f"{stem}.{impl}.wav")
        p = subprocess.run([os.path.join(T, "build/aac-dec"), os.path.join(T, src), dst],
                           capture_output=True)
        data = open(dst, "rb").read() if os.path.exists(dst) else b""
        sr = ch = bits = data_len = 0
        if len(data) >= 44 and data[:4] == b"RIFF" and data[8:12] == b"WAVE":
            off = 12
            while off + 8 <= len(data):
                cid = data[off:off + 4]
                sz = struct.unpack_from("<I", data, off + 4)[0]
                body = off + 8
                if cid == b"fmt " and sz >= 16:
                    _, ch, sr, _, _, bits = struct.unpack_from("<HHIIHH", data, body)
                elif cid == b"data":
                    data_len = sz
                off = body + sz + (sz & 1)
        samples = data_len // (ch * 2) if ch else 0
        out.append({
            "stem": stem, "impl": impl, "decoder_rc": p.returncode,
            "stderr_bytes": len(p.stderr), "decoded_wav_bytes": len(data),
            "sample_rate": sr, "channels": ch, "bits": bits,
            "decoded_samples_per_ch": samples,
            "decoded_duration_s": samples / sr if sr else 0,
            "input_duration_s": float(r["audio_s"]),
            "adts_frames": r["frames"],
        })
with open(os.path.join(T, "analysis/decode-validation.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(out[0]), delimiter="\t")
    w.writeheader()
    w.writerows(out)
bad = [x for x in out if x["decoder_rc"] != 0 or x["stderr_bytes"] != 0 or x["decoded_wav_bytes"] == 0]
print("decode_rows", len(out), "bad", len(bad))
by = {}
for x in out:
    by.setdefault(x["stem"], {})[x["impl"]] = x
mismatch = [s for s, d in by.items()
            if d["c"]["decoded_samples_per_ch"] != d["r"]["decoded_samples_per_ch"]]
print("sample_count_mismatches", mismatch)
