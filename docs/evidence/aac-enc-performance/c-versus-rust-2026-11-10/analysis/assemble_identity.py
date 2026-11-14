#!/usr/bin/env python3
"""Assemble corpus manifest and source/binary identity records."""
import csv
import hashlib
import os
import subprocess

T = "/private/var/folders/*/T/opencode/aac-c-vs-rust-2026-11-10"
SRC = "/Users/USER/backups/fdk-aac-m5.23-worktree-20261016T193606Z/worktree"
OUT = os.path.join(T, "analysis")
REPO = "/Users/USER/Documents/VSCode/fdk-aac/fdk-aac"


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def aggregate(root, rel_paths):
    h = hashlib.sha256()
    for rel in sorted(rel_paths):
        path = os.path.join(root, rel)
        with open(path, "rb") as f:
            data = f.read()
        h.update(rel.encode())
        h.update(b"\0")
        h.update(len(data).to_bytes(8, "little"))
        h.update(data)
    return h.hexdigest()


def walk(root, sub):
    base = os.path.join(root, sub)
    out = []
    for dirpath, _, files in os.walk(base):
        for name in files:
            out.append(os.path.relpath(os.path.join(dirpath, name), root))
    return out


# --- corpus manifest -------------------------------------------------------
dump = {}
for line in open(os.path.join(T, "logs/fixture-dump.log")):
    p = line.rstrip("\n").split("\t")
    if p and p[0] == "DUMP":
        dump[p[1]] = {
            "sr": int(p[2]), "ch": int(p[3]), "samples": int(p[4]),
            "pcm_fnv": p[5],
        }

expected = {}
with open(os.path.join(REPO, "docs/evidence/aac-enc-performance/aggregate-performance-checkpoint-2026-10-28/workloads.tsv")) as f:
    for row in csv.DictReader(f, delimiter="\t"):
        expected[row["workload"]] = row

valid = {}
with open(os.path.join(OUT, "output-validation.tsv")) as f:
    for row in csv.DictReader(f, delimiter="\t"):
        valid[row["stem"]] = row

category = {
    "m2-click-mono-48k": "short_diagnostic", "m2-edge-mono-48k": "short_diagnostic",
    "m2-impulse-mono-48k": "short_diagnostic", "m2-onset-stereo-48k": "short_diagnostic",
    "m2-silence-stereo-48k": "short_diagnostic", "m2-sine-stereo-48k": "short_diagnostic",
    "m2-stepdown-stereo-48k": "short_diagnostic",
    "m3-multitone-stereo-48k": "tonal_noise", "m3-noise-mono-48k": "tonal_noise",
    "m3-quiet-sine-stereo-48k": "tonal_noise", "m3-sine-stereo-441k": "tonal_noise",
    "m3-sweep-stereo-48k": "tonal_noise", "m3-tonal-noise-stereo-48k": "tonal_noise",
    "m3-onset-stereo-441k": "transient", "m6-stress-stereo-48k": "transient",
    "m7-music-stereo-48k": "music", "m8-music-eroica-stereo-48k": "music",
    "m7-speech-clean-stereo-48k": "speech", "m7-speech-pauses-stereo-48k": "speech",
    "m7-music-stereo-48k-x4": "long",
}

order = [
    "m2-silence-stereo-48k", "m2-sine-stereo-48k", "m2-onset-stereo-48k",
    "m2-stepdown-stereo-48k", "m2-click-mono-48k", "m2-impulse-mono-48k",
    "m2-edge-mono-48k", "m3-multitone-stereo-48k", "m3-sweep-stereo-48k",
    "m3-noise-mono-48k", "m3-tonal-noise-stereo-48k", "m3-quiet-sine-stereo-48k",
    "m3-sine-stereo-441k", "m3-onset-stereo-441k", "m6-stress-stereo-48k",
    "m7-music-stereo-48k", "m7-speech-pauses-stereo-48k",
    "m7-speech-clean-stereo-48k", "m8-music-eroica-stereo-48k",
    "m7-music-stereo-48k-x4",
]
manifest_rows = []
total_audio = 0.0
for stem in order:
    d = dump[stem]
    wav = os.path.join(T, "corpus", stem + ".wav")
    v = valid[stem]
    audio_s = d["samples"] / d["sr"]
    total_audio += audio_s
    manifest_rows.append({
        "stem": stem,
        "category": category[stem],
        "wav": wav,
        "wav_sha256": sha256_file(wav),
        "pcm_fnv1a64": d["pcm_fnv"],
        "sample_rate": d["sr"],
        "channels": d["ch"],
        "samples_per_ch": d["samples"],
        "audio_s": f"{audio_s:.6f}",
        "frames": v["frames"],
        "published_rust_bytes": expected[stem]["bytes"],
        "published_rust_adts_fnv1a64": expected[stem]["hash"],
        "c_bytes": v["c_bytes"],
        "r_bytes": v["r_bytes"],
        "c_sha256": v["c_sha256"],
        "r_sha256": v["r_sha256"],
    })
with open(os.path.join(OUT, "corpus-manifest.tsv"), "w", newline="") as f:
    w = csv.DictWriter(f, fieldnames=list(manifest_rows[0]), delimiter="\t")
    w.writeheader()
    w.writerows(manifest_rows)

# --- identity --------------------------------------------------------------
c_rel = walk(SRC, "cpp")
for name in ("aac-enc.c", "aac-dec.c", "wavreader.c", "wavreader.h", "CMakeLists.txt"):
    c_rel.append(name)
c_agg = aggregate(SRC, c_rel)
rust_head = os.path.join(T, "head-reference")
rust_rel = walk(rust_head, "rust/src")
rust_rel += ["rust/Cargo.toml", "rust/Cargo.lock"]
rust_agg = aggregate(rust_head, rust_rel)

key_c = ["cpp/libAACenc/src/aacenc.cpp", "cpp/libAACenc/src/aacenc_lib.cpp",
         "cpp/libAACenc/src/block_switch.cpp", "cpp/libAACenc/src/quantize.cpp",
         "cpp/libAACenc/src/psy_main.cpp", "cpp/libAACenc/src/bitenc.cpp",
         "aac-enc.c", "wavreader.c", "wavreader.h", "CMakeLists.txt"]
key_rust = ["rust/Cargo.toml", "rust/Cargo.lock", "rust/src/lib.rs",
            "rust/src/aac_enc/encoder.rs", "rust/src/aac_enc/quant.rs",
            "rust/src/aac_enc/scf.rs", "rust/src/aac_enc/bitenc.rs",
            "rust/src/aac_enc/bitcount.rs", "rust/src/aac_enc/tns.rs"]

lines = []
lines.append("# Source and binary identity (2026-11-10 C-vs-Rust benchmark)")
lines.append("")
lines.append("## Repository")
lines.append(f"head_commit\t1bd724fa8786ef4003a1513b410400674bd88f52")
lines.append(f"head_tree\t042059f886362950f71529706d9b391f12754547")
lines.append("working_branch\tmain")
lines.append("origin_main\t1bd724fa8786ef4003a1513b410400674bd88f52")
lines.append("")
lines.append("## C reference source")
lines.append(f"source_root\t{SRC}")
lines.append("ferrite_backup_commit\te4258d2a5b55581f5f2b27aa6c1a369bb2503e43")
lines.append("pinned_reference_commit\tf14ad40a574a4132df947d15e9fb14bee91cc8a5")
lines.append("reference_capture\tevidence/m522/base (byte-identical cpp/ and CLI files)")
lines.append(f"source_aggregate_sha256\t{c_agg}")
lines.append(f"source_file_count\t{len(c_rel)}")
for rel in key_c:
    lines.append(f"sha256\t{rel}\t{sha256_file(os.path.join(SRC, rel))}")
lines.append("")
lines.append("## Rust production source")
lines.append(f"source_aggregate_sha256\t{rust_agg}")
lines.append(f"source_file_count\t{len(rust_rel)}")
for rel in key_rust:
    lines.append(f"sha256\t{rel}\t{sha256_file(os.path.join(rust_head, rel))}")
lines.append(f"benchmark_example\t{sha256_file(os.path.join(T, 'src/ferrite/rust/examples/bench_corpus.rs'))}")
lines.append(f"fixture_dump\t{sha256_file(os.path.join(T, 'src/ferrite/rust/src/aac_enc/fixture_dump.rs'))}")
lines.append("production_source_difference\tHEAD plus the two benchmark-only files and one test-module registration; no encoder source change")
lines.append("")
lines.append("## Binaries and harnesses")
for label, path in [
    ("c_harness_native", os.path.join(T, "bin/c_bench_corpus")),
    ("c_library_native", os.path.join(T, "build/libfdk-aac.2.0.4.dylib")),
    ("c_aac_enc", os.path.join(T, "build/aac-enc")),
    ("c_aac_dec", os.path.join(T, "build/aac-dec")),
    ("c_harness_portable", os.path.join(T, "bin/c_bench_corpus_portable")),
    ("c_library_portable", os.path.join(T, "build-portable/libfdk-aac.2.0.4.dylib")),
    ("rust_harness_native", os.path.join(T, "rust-target/release/examples/bench_corpus")),
    ("rust_harness_portable", os.path.join(T, "rust-target-portable/release/examples/bench_corpus")),
]:
    lines.append(f"{label}\t{sha256_file(path)}\t{os.path.getsize(path)}")
with open(os.path.join(OUT, "identity.txt"), "w") as f:
    f.write("\n".join(lines) + "\n")

print("\n".join(lines[:20]))
print(f"corpus_rows={len(manifest_rows)} total_audio_s={total_audio:.6f}")
