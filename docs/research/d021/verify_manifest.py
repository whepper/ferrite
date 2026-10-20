#!/usr/bin/env python3
"""D-021 archive verifier (read-only; stdlib only; modifies nothing).

Usage from the repository root:
    python3 docs/research/d021/verify_manifest.py

Checks every manifest.json `files` entry for presence and SHA-256 match,
lists `missing` entries for awareness, and confirms the archive documents
exist. Exit 0 only when every check passes.
"""
import hashlib
import json
import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)))


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    with open(os.path.join(ROOT, "manifest.json")) as f:
        manifest = json.load(f)
    failures = []
    checked = 0
    for entry in manifest["files"]:
        path = os.path.join(ROOT, entry["archive_path"])
        if not os.path.isfile(path):
            failures.append(f"ABSENT: {entry['archive_path']}")
            continue
        checked += 1
        if sha256(path) != entry["archived_sha256"]:
            failures.append(f"HASH MISMATCH: {entry['archive_path']}")
        if entry["original_sha256"] != entry["archived_sha256"]:
            failures.append(f"COPY DRIFT: {entry['archive_path']}")
    for doc in manifest.get("archive_documents", []):
        if not os.path.isfile(os.path.join(ROOT, doc)):
            failures.append(f"ABSENT DOCUMENT: {doc}")
    print(f"checked={checked} failures={len(failures)}")
    for entry in manifest.get("missing", []):
        print(f"not-archived: {entry.get('original_path')} [{entry.get('provenance')}]")
    for failure in failures:
        print(failure)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
