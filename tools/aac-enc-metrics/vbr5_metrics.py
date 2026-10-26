#!/usr/bin/env python3
"""AAC-LC VBR-5 C-vs-Rust objective metrics (measurement only).

This tool is a maintained, parameterized successor to the scratch V3
`stage1_metrics.py`. It deliberately does **not** implement perceptual
metrics, listening/ABX tests, per-frame/per-band localization, or quality
pass/fail thresholds.

Measurements:
  * ADTS framing: total bytes, frame count, header/payload split, frame-size
    distribution, bitrate over the original fixture duration.
  * Structural decode: exit status, empty stderr, WAV sample rate/channels/
    bits, decoded length, decoded WAV hash.
  * Decoded full-band error vs the original PCM: SNR, RMS error, peak error,
    overlap, clipping. `sdr_db` is intentionally the same number as
    `snr_db` and is marked `sdr_db_is_alias`.
  * Rust-vs-C decoded error at lag 0.

Alignment:
  * Independent per-channel search around the documented nominal lag.
  * Stereo channel estimates must agree within +/-1 sample.
  * Ambiguous or disagreeing streams are excluded from quality metrics.
  * Only the selected lag is applied to the decoded WAV; the decoder's
    outputDelay is recorded but never subtracted a second time.

Eligibility is an evidence scope, not a quality score:
  * gated: 48 kHz, approved/deterministic fixture provenance, verified
    provenance for both the Rust and C encoded streams, a plausible decoded
    length, and an unambiguous per-channel alignment.
  * exploratory: measured but outside one of those gates (44.1 kHz gate,
    provisional m8, unverified pre-generated ADTS, or an explicit reference
    override).
  * excluded: measured structurally but not eligible for quality metrics
    because alignment is ambiguous/disagreeing or the decoded length is
    implausible.

Stream provenance is never inferred from a supplied directory or an encoder
binary hash alone. A gated stream must either have been produced by a verified
encoder/harness in this invocation or match a retained V3 artifact byte for
byte.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import wave
from pathlib import Path
from typing import Any

TOOL_NAME = "ferrite-aac-vbr5-metrics"
TOOL_VERSION = "1.1.0"
MIN_OVERLAP_SAMPLES = 1000


class MetricsError(RuntimeError):
    """A clear, actionable measurement failure."""


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise MetricsError(f"missing JSON file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise MetricsError(f"invalid JSON in {path}: {exc}") from exc


def run_command(
    cmd: list[str],
    *,
    cwd: Path | None = None,
    timeout: int = 600,
    allow_stderr: bool = True,
) -> subprocess.CompletedProcess[str]:
    try:
        cp = subprocess.run(
            cmd,
            cwd=str(cwd) if cwd else None,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except FileNotFoundError as exc:
        raise MetricsError(f"required executable not found: {cmd[0]}") from exc
    if cp.returncode != 0:
        raise MetricsError(
            "command failed\n"
            f"  command: {' '.join(cmd)}\n"
            f"  cwd: {cwd or os.getcwd()}\n"
            f"  exit: {cp.returncode}\n"
            f"  stdout: {cp.stdout[-2000:]}\n"
            f"  stderr: {cp.stderr[-4000:]}"
        )
    if not allow_stderr and cp.stderr.strip():
        raise MetricsError(
            "command wrote to stderr, which is a failure under the decoder/encoder gate\n"
            f"  command: {' '.join(cmd)}\n"
            f"  stderr: {cp.stderr[-4000:]}"
        )
    return cp


def read_wav_i16(path: Path) -> dict[str, Any]:
    try:
        with wave.open(str(path), "rb") as w:
            channels = w.getnchannels()
            sample_rate = w.getframerate()
            samples = w.getnframes()
            sample_width = w.getsampwidth()
            if sample_width != 2:
                raise MetricsError(f"{path}: expected 16-bit PCM, got {sample_width * 8}-bit")
            raw = w.readframes(samples)
    except (wave.Error, EOFError) as exc:
        raise MetricsError(f"{path}: cannot read WAV: {exc}") from exc
    expected = samples * channels * 2
    if len(raw) != expected:
        raise MetricsError(f"{path}: truncated PCM ({len(raw)} != {expected} bytes)")
    values = struct_unpack_i16(raw)
    pcm = [[values[i * channels + c] for i in range(samples)] for c in range(channels)]
    return {
        "sample_rate_hz": sample_rate,
        "channels": channels,
        "samples_per_channel": samples,
        "bits_per_sample": 16,
        "pcm": pcm,
    }


def struct_unpack_i16(raw: bytes) -> tuple[int, ...]:
    # Kept as a tiny helper so read_wav_i16 stays readable without importing
    # struct in several places.
    import struct

    return struct.unpack("<%dh" % (len(raw) // 2), raw)


def parse_adts(path: Path) -> dict[str, Any]:
    data = path.read_bytes()
    lengths: list[int] = []
    header_bytes = 0
    offset = 0
    while offset + 7 <= len(data):
        if data[offset] != 0xFF or (data[offset + 1] & 0xF0) != 0xF0:
            raise MetricsError(f"{path}: ADTS sync lost at byte {offset}")
        frame_length = (
            ((data[offset + 3] & 0x03) << 11)
            | (data[offset + 4] << 3)
            | (data[offset + 5] >> 5)
        )
        header_length = 7 if (data[offset + 1] & 0x01) else 9
        if frame_length < header_length:
            raise MetricsError(f"{path}: invalid ADTS frame length {frame_length} at {offset}")
        lengths.append(frame_length)
        header_bytes += header_length
        offset += frame_length
    if offset != len(data):
        raise MetricsError(f"{path}: trailing bytes after last ADTS frame ({len(data) - offset})")
    if not lengths:
        raise MetricsError(f"{path}: no ADTS frames")
    return {
        "total_bytes": len(data),
        "frame_count": len(lengths),
        "header_bytes": header_bytes,
        "payload_bytes": len(data) - header_bytes,
        "frame_length_bytes": frame_stats(lengths),
        "adts_sha256": hashlib.sha256(data).hexdigest(),
    }


def frame_stats(values: list[int]) -> dict[str, Any]:
    ordered = sorted(values)
    n = len(ordered)

    def percentile(q: float) -> float:
        i = (n - 1) * q / 100.0
        lo = int(math.floor(i))
        hi = int(math.ceil(i))
        return ordered[lo] + (ordered[hi] - ordered[lo]) * (i - lo)

    return {
        "count": n,
        "min_bytes": ordered[0],
        "p50_bytes": percentile(50.0),
        "p95_bytes": percentile(95.0),
        "max_bytes": ordered[-1],
        "mean_bytes": sum(ordered) / n,
    }


def normalized_correlation(
    reference: list[int],
    test: list[int],
    lag: int,
    step: int,
) -> float | None:
    """Normalized correlation for lag L: test[L:] vs reference[0:].

    Positive L means the decoded/test signal starts later than the original.
    Negative L is supported for diagnostics. The overlap is never padded.
    """
    if lag >= 0:
        ref_start = 0
        test_start = lag
    else:
        ref_start = -lag
        test_start = 0
    count = min(len(test) - test_start, len(reference) - ref_start)
    if count < MIN_OVERLAP_SAMPLES:
        return None
    ref_energy = 0.0
    test_energy = 0.0
    cross = 0.0
    # Local bindings keep the pure-Python loop reasonably fast.
    ref = reference
    tst = test
    rs = ref_start
    ts = test_start
    for i in range(0, count, step):
        x = ref[rs + i]
        y = tst[ts + i]
        ref_energy += float(x) * x
        test_energy += float(y) * y
        cross += float(x) * y
    if ref_energy <= 0.0 or test_energy <= 0.0:
        return None
    return cross / math.sqrt(ref_energy * test_energy)


def classify_lag_candidates(
    full: list[tuple[int, float]],
    coarse: list[tuple[int, float]],
    *,
    lower: int,
    upper: int,
    tie_epsilon: float,
) -> dict[str, Any]:
    """Classify full-rate lag candidates and their runner-up margin."""
    best_lag, best_score = max(full, key=lambda p: p[1])
    second_pool = [p for p in full if abs(p[0] - best_lag) > 1]
    if not second_pool:
        second_pool = [p for p in coarse if abs(p[0] - best_lag) > 1]
    second_lag: int | None = None
    second_score: float | None = None
    if second_pool:
        second_lag, second_score = max(second_pool, key=lambda p: p[1])
    score_margin = None if second_score is None else best_score - second_score

    if best_lag <= lower or best_lag >= upper:
        status = "search_boundary"
    elif score_margin is not None and score_margin <= tie_epsilon:
        status = "score_tie"
    else:
        status = "unique"

    return {
        "estimated_lag_samples": best_lag,
        "best_score": round(best_score, 12),
        "second_best_lag_samples": second_lag,
        "second_best_score": None if second_score is None else round(second_score, 12),
        "score_margin": None if score_margin is None else round(score_margin, 12),
        "status": status,
    }


def estimate_channel_lag(
    reference: list[int],
    test: list[int],
    *,
    nominal_lag: int,
    search_half_width: int,
    coarse_step: int,
    refine_half_width: int,
    tie_epsilon: float,
) -> dict[str, Any]:
    lower = nominal_lag - search_half_width
    upper = nominal_lag + search_half_width

    coarse: list[tuple[int, float]] = []
    for lag in range(lower, upper + 1, coarse_step):
        score = normalized_correlation(reference, test, lag, coarse_step)
        if score is not None:
            coarse.append((lag, score))
    if not coarse:
        return {
            "estimated_lag_samples": None,
            "best_score": None,
            "second_best_lag_samples": None,
            "second_best_score": None,
            "score_margin": None,
            "status": "no_signal",
            "coarse_step_samples": coarse_step,
            "refine_half_width_samples": refine_half_width,
        }

    coarse_best_lag, _ = max(coarse, key=lambda p: p[1])
    refine_span = refine_half_width
    full: list[tuple[int, float]] = []
    for lag in range(
        max(lower, coarse_best_lag - refine_span),
        min(upper, coarse_best_lag + refine_span) + 1,
    ):
        score = normalized_correlation(reference, test, lag, 1)
        if score is not None:
            full.append((lag, score))
    if not full:
        return {
            "estimated_lag_samples": None,
            "best_score": None,
            "second_best_lag_samples": None,
            "second_best_score": None,
            "score_margin": None,
            "status": "no_signal",
            "coarse_step_samples": coarse_step,
            "refine_half_width_samples": refine_half_width,
        }

    classified = classify_lag_candidates(
        full,
        coarse,
        lower=lower,
        upper=upper,
        tie_epsilon=tie_epsilon,
    )
    return {
        **classified,
        "coarse_step_samples": coarse_step,
        "refine_half_width_samples": refine_half_width,
    }


def estimate_alignment(
    original: dict[str, Any],
    decoded: dict[str, Any],
    contract: dict[str, Any],
) -> dict[str, Any]:
    nominal = int(contract["nominal_lag_samples"])
    half = int(contract["search_half_width_samples"])
    channel_tolerance = int(contract["channel_tolerance_samples"])
    nominal_tolerance = int(contract["nominal_tolerance_samples"])
    tie_epsilon = float(contract["tie_epsilon"])
    threshold = int(contract["short_signal_threshold_samples"])
    short_step = int(contract["short_signal_coarse_step_samples"])
    long_step = int(contract["coarse_step_samples"])
    short_refine = int(contract["refine_half_width_samples"])
    long_refine = int(
        contract.get(
            "long_signal_refine_half_width_samples",
            max(short_refine, long_step // 2),
        )
    )

    channels = int(original["channels"])
    samples = int(original["samples_per_channel"])
    long_signal = samples > threshold
    coarse_step = long_step if long_signal else short_step
    refine_half = long_refine if long_signal else short_refine

    per_channel: list[dict[str, Any]] = []
    for channel in range(channels):
        result = estimate_channel_lag(
            original["pcm"][channel],
            decoded["pcm"][channel],
            nominal_lag=nominal,
            search_half_width=half,
            coarse_step=coarse_step,
            refine_half_width=refine_half,
            tie_epsilon=tie_epsilon,
        )
        result["channel"] = channel
        per_channel.append(result)

    lags = [p["estimated_lag_samples"] for p in per_channel]
    channel_delta: int | None = None
    if channels > 1 and all(v is not None for v in lags):
        channel_delta = max(lags) - min(lags)

    ambiguity_status = "unique"
    exclusion_reason: str | None = None
    selected_lag: int | None = None
    selection_method: str | None = None

    for item in per_channel:
        if item["status"] != "unique":
            ambiguity_status = item["status"]
            exclusion_reason = f"channel {item['channel']}: {item['status']}"
            break
    if exclusion_reason is None and channel_delta is not None and channel_delta > channel_tolerance:
        ambiguity_status = "channel_lag_disagreement"
        exclusion_reason = (
            f"per-channel estimates {lags} differ by {channel_delta} samples "
            f"(tolerance {channel_tolerance})"
        )

    if exclusion_reason is None:
        assert all(v is not None for v in lags)
        if all(abs(v - nominal) <= nominal_tolerance for v in lags):
            selected_lag = nominal
            selection_method = "nominal_prior_within_tolerance"
        else:
            candidates = range(min(lags), max(lags) + 1)
            scored: list[tuple[int, float]] = []
            for candidate in candidates:
                scores = [
                    normalized_correlation(
                        original["pcm"][channel],
                        decoded["pcm"][channel],
                        candidate,
                        1,
                    )
                    for channel in range(channels)
                ]
                defined = [s for s in scores if s is not None]
                if defined:
                    scored.append((candidate, sum(defined) / len(defined)))
            if not scored:
                ambiguity_status = "no_signal"
                exclusion_reason = "no defined joint correlation candidate"
            else:
                best_score = max(score for _, score in scored)
                best_candidates = [lag for lag, score in scored if score == best_score]
                selected_lag = min(best_candidates, key=lambda v: (abs(v - nominal), v))
                selection_method = "joint_best_within_channel_tolerance"
        if selected_lag is not None and abs(selected_lag - nominal) > nominal_tolerance:
            ambiguity_status = "outside_nominal_tolerance"
            exclusion_reason = (
                f"selected lag {selected_lag} is outside nominal {nominal} "
                f"+/-{nominal_tolerance}"
            )
            selected_lag = None

    return {
        "nominal_lag_samples": nominal,
        "search_range_samples": [nominal - half, nominal + half],
        "channel_tolerance_samples": channel_tolerance,
        "nominal_tolerance_samples": nominal_tolerance,
        "coarse_step_samples": coarse_step,
        "per_channel": per_channel,
        "channel_lag_delta_samples": channel_delta,
        "selected_lag_samples": selected_lag,
        "selection_method": selection_method,
        "ambiguity_status": ambiguity_status,
        "ambiguous": selected_lag is None,
        "excluded_from_quality_metrics": selected_lag is None,
        "exclusion_reason": exclusion_reason,
    }


def error_stats(reference: list[int], test: list[int]) -> dict[str, Any]:
    n = len(reference)
    if n == 0:
        raise MetricsError("cannot compute error over an empty overlap")
    ref_energy = 0.0
    error_energy = 0.0
    peak_error = 0
    signal_peak = 0
    for a, b in zip(reference, test):
        ref_energy += float(a) * a
        diff = a - b
        error_energy += float(diff) * diff
        if abs(diff) > peak_error:
            peak_error = abs(diff)
        if abs(a) > signal_peak:
            signal_peak = abs(a)

    if ref_energy == 0.0:
        snr_db = None
        snr_status = "undefined_silence"
    elif error_energy == 0.0:
        snr_db = None
        snr_status = "identical"
    else:
        snr_db = 10.0 * math.log10(ref_energy / error_energy)
        snr_status = "defined"

    return {
        "snr_db": None if snr_db is None else round(snr_db, 6),
        "sdr_db": None if snr_db is None else round(snr_db, 6),
        "sdr_db_is_alias": True,
        "snr_status": snr_status,
        "rms_error_lsb": round(math.sqrt(error_energy / n), 6),
        "peak_error_lsb": peak_error,
        "signal_rms_lsb": round(math.sqrt(ref_energy / n), 6),
        "signal_peak_lsb": signal_peak,
        "samples_compared": n,
    }


def measure_pair(
    reference: dict[str, Any],
    test: dict[str, Any],
    *,
    lag_samples: int,
    reference_name: str,
    test_name: str,
) -> dict[str, Any]:
    channels = int(reference["channels"])
    if int(test["channels"]) != channels:
        raise MetricsError(
            f"channel mismatch: {reference_name}={channels}, {test_name}={test['channels']}"
        )
    if lag_samples >= 0:
        ref_offset = 0
        test_offset = lag_samples
    else:
        ref_offset = -lag_samples
        test_offset = 0
    overlap = min(
        int(test["samples_per_channel"]) - test_offset,
        int(reference["samples_per_channel"]) - ref_offset,
    )
    if overlap <= 0:
        raise MetricsError(f"non-positive overlap for {reference_name} vs {test_name}")

    per_channel: list[dict[str, Any]] = []
    aggregate_ref: list[int] = []
    aggregate_test: list[int] = []
    for channel in range(channels):
        ref_slice = reference["pcm"][channel][ref_offset : ref_offset + overlap]
        test_slice = test["pcm"][channel][test_offset : test_offset + overlap]
        aggregate_ref.extend(ref_slice)
        aggregate_test.extend(test_slice)
        stats = error_stats(ref_slice, test_slice)
        stats["channel"] = channel
        per_channel.append(stats)

    aggregate = error_stats(aggregate_ref, aggregate_test)
    reference_tail = int(reference["samples_per_channel"]) - ref_offset - overlap
    test_tail = int(test["samples_per_channel"]) - test_offset - overlap
    return {
        "reference": reference_name,
        "test": test_name,
        "lag_samples": lag_samples,
        "overlap_samples_per_channel": overlap,
        "overlap_samples_total": overlap * channels,
        "reference_tail_shortfall_samples_per_channel": reference_tail,
        "test_tail_shortfall_samples_per_channel": test_tail,
        "per_channel": per_channel,
        "aggregate": aggregate,
    }


def validate_manifest(manifest: dict[str, Any]) -> None:
    if manifest.get("schema_version") != 1:
        raise MetricsError("manifest schema_version must be 1")
    alignment = manifest.get("alignment")
    if not isinstance(alignment, dict):
        raise MetricsError("manifest alignment must be an object")
    for key in (
        "nominal_lag_samples",
        "search_half_width_samples",
        "channel_tolerance_samples",
        "nominal_tolerance_samples",
        "tie_epsilon",
        "coarse_step_samples",
        "short_signal_threshold_samples",
        "short_signal_coarse_step_samples",
        "refine_half_width_samples",
    ):
        if key not in alignment:
            raise MetricsError(f"manifest alignment is missing {key}")
    long_refine = alignment.get("long_signal_refine_half_width_samples")
    if long_refine is None:
        raise MetricsError(
            "manifest alignment is missing long_signal_refine_half_width_samples"
        )
    if int(long_refine) < math.ceil(int(alignment["coarse_step_samples"]) / 2):
        raise MetricsError(
            "long_signal_refine_half_width_samples must cover at least half of "
            "the long-signal coarse stride"
        )
    frame_samples = manifest.get("encoder_configuration", {}).get("frame_length_samples")
    if frame_samples != 1024:
        raise MetricsError("manifest encoder frame_length_samples must be 1024")
    delays = manifest.get("decoder_output_delay_samples")
    if not isinstance(delays, dict) or not delays:
        raise MetricsError("manifest decoder_output_delay_samples must be a non-empty object")
    for rate in ("48000", "44100"):
        if int(delays.get(rate, -1)) < 0:
            raise MetricsError(f"manifest decoder_output_delay_samples is missing {rate}")
    fixtures = manifest.get("fixtures")
    if not isinstance(fixtures, list):
        raise MetricsError("manifest fixtures must be a list")
    stems = [f.get("stem") for f in fixtures]
    if len(set(stems)) != len(stems):
        raise MetricsError("manifest fixture stems are not unique")
    corpus = [f for f in fixtures if f.get("corpus_member") is True]
    separate = [f for f in fixtures if f.get("corpus_member") is False]
    if len(corpus) != 19:
        raise MetricsError(f"manifest must contain 19 corpus members, found {len(corpus)}")
    if len(separate) != 1:
        raise MetricsError(f"manifest must contain exactly one separate probe, found {len(separate)}")
    if manifest.get("corpus_member_count") != 19:
        raise MetricsError("manifest corpus_member_count must be 19")
    if manifest.get("separate_probe_count") != 1:
        raise MetricsError("manifest separate_probe_count must be 1")

    synthetic = [f for f in corpus if f.get("category") == "synthetic"]
    licensed = [
        f
        for f in corpus
        if f.get("category") in ("licensed-speech", "licensed-music")
    ]
    if len(synthetic) != 15:
        raise MetricsError(f"manifest must contain 15 synthetic corpus members, found {len(synthetic)}")
    if len(licensed) != 3:
        raise MetricsError(f"manifest must contain 3 licensed corpus members, found {len(licensed)}")
    if not any(f.get("stem") == "m6-stress-stereo-48k" for f in synthetic):
        raise MetricsError("manifest is missing m6-stress-stereo-48k")
    m8 = [f for f in corpus if f.get("stem") == "m8-music-eroica-stereo-48k"]
    if len(m8) != 1 or m8[0].get("provenance_status") != "provisional":
        raise MetricsError("m8-music-eroica-stereo-48k must be present and marked provisional")
    if m8[0].get("eligibility", {}).get("gated_aggregate") is not False:
        raise MetricsError("provisional m8 must not be gated by default")

    for fixture in fixtures:
        for key in (
            "stem",
            "category",
            "provenance_status",
            "provenance_source",
            "license",
            "corpus_member",
            "sample_rate_hz",
            "channels",
            "samples_per_channel",
            "duration_seconds",
            "input_sha256",
            "input_source",
            "alignment_gate",
            "eligibility",
        ):
            if key not in fixture:
                raise MetricsError(f"{fixture.get('stem', '<unknown>')}: missing field {key}")
        if len(str(fixture["input_sha256"])) != 64:
            raise MetricsError(f"{fixture['stem']}: input_sha256 is not 64 hex characters")
        if fixture["sample_rate_hz"] == 44100 and fixture["eligibility"]["gated_aggregate"]:
            raise MetricsError(
                f"{fixture['stem']}: 44.1 kHz fixture must not be gated before the separate gate"
            )

    mono = separate[0]
    if mono.get("corpus_member") is not False:
        raise MetricsError("separate probe must have corpus_member=false")
    if mono.get("stem") != "m3-onset-mono-441k":
        raise MetricsError("separate probe must be m3-onset-mono-441k")
    if mono.get("sample_rate_hz") != 44100 or mono.get("channels") != 1:
        raise MetricsError("separate probe must be 44.1 kHz mono")
    hint = mono.get("input_path_hint")
    if hint is not None and not isinstance(hint, str):
        raise MetricsError("separate probe input_path_hint must be a string when present")


def selected_fixtures(
    manifest: dict[str, Any],
    only: list[str] | None,
) -> list[dict[str, Any]]:
    fixtures = [f for f in manifest["fixtures"] if f["corpus_member"]]
    if only:
        wanted = set(only)
        known = {f["stem"] for f in fixtures}
        unknown = wanted - known
        if unknown:
            raise MetricsError(
                f"--only contains non-corpus or unknown stems: {sorted(unknown)}"
            )
        fixtures = [f for f in fixtures if f["stem"] in wanted]
    if not fixtures:
        raise MetricsError("no corpus fixtures selected")
    return fixtures


def prepare_inputs(
    fixtures: list[dict[str, Any]],
    *,
    repo_root: Path,
    work_dir: Path,
) -> dict[str, Path]:
    inputs_dir = work_dir / "inputs"
    inputs_dir.mkdir(parents=True, exist_ok=True)
    result: dict[str, Path] = {}

    synthetic = [
        f
        for f in fixtures
        if str(f["input_source"]).startswith("generated:tools/enc-oracle/make_fixtures.py")
    ]
    if synthetic:
        generator = repo_root / "tools" / "enc-oracle" / "make_fixtures.py"
        if not generator.is_file():
            raise MetricsError(f"missing fixture generator: {generator}")
        cp = run_command(
            [sys.executable, str(generator), str(inputs_dir)],
            cwd=repo_root,
            timeout=120,
            allow_stderr=True,
        )
        (work_dir / "fixture-generation.log").write_text(
            cp.stdout + cp.stderr, encoding="utf-8"
        )

    for fixture in fixtures:
        stem = fixture["stem"]
        source = str(fixture["input_source"])
        if source.startswith("generated:tools/enc-oracle/make_fixtures.py"):
            path = inputs_dir / f"{stem}.wav"
        elif source.startswith("committed:"):
            committed = repo_root / source.split(":", 1)[1]
            if not committed.is_file():
                raise MetricsError(f"missing committed fixture: {committed}")
            path = inputs_dir / f"{stem}.wav"
            if path.resolve() != committed.resolve():
                shutil.copyfile(committed, path)
        elif source.startswith("generated:m5b_dump_inputs"):
            raise MetricsError(
                f"{stem}: the separate mono/44.1 kHz probe is identified in the manifest "
                "but is not part of the 19-fixture V3 run; no replacement input is used"
            )
        else:
            raise MetricsError(f"{stem}: unsupported input_source {source!r}")

        if not path.is_file():
            raise MetricsError(f"missing prepared input: {path}")
        actual = sha256_file(path)
        expected = fixture["input_sha256"]
        if actual != expected:
            raise MetricsError(
                f"{stem}: input SHA-256 mismatch\n"
                f"  expected: {expected}\n"
                f"  actual:   {actual}\n"
                f"  file:     {path}"
            )
        result[stem] = path

    return result


RUST_BUILD_ENV_KEYS = (
    "RUSTFLAGS",
    "CARGO_BUILD_RUSTFLAGS",
    "CARGO_PROFILE_TEST_OPT_LEVEL",
    "CARGO_PROFILE_DEV_OPT_LEVEL",
    "CARGO_PROFILE_TEST_DEBUG",
    "CARGO_PROFILE_DEV_DEBUG",
    "CARGO_INCREMENTAL",
    "CARGO_BUILD_TARGET",
)


def toolchain_matches_declared(
    declared: str, rustc_version: str, cargo_version: str
) -> bool | None:
    """Compare the declared toolchain version(s) with the measured ones.

    The manifest records a compact string such as ``rustc/cargo 1.99.1``.
    Returns None when no version can be parsed from the declaration.
    """
    import re

    versions = re.findall(r"\d+\.\d+\.\d+", str(declared))
    if not versions:
        return None
    return all(v in rustc_version and v in cargo_version for v in versions)


def run_git_stdout(repo: Path, args: list[str], *, role: str) -> str:
    try:
        return run_command(
            ["git", "-C", str(repo), *args], timeout=30
        ).stdout
    except MetricsError as exc:
        raise MetricsError(f"cannot determine {role} source identity: {exc}") from exc


def expand_path_hint(value: str) -> Path:
    return Path(os.path.expandvars(os.path.expanduser(value)))


def verify_separate_probes(
    manifest: dict[str, Any],
    *,
    explicit_path: Path | None = None,
) -> list[dict[str, Any]]:
    """Validate the separate probe hash when its input is available.

    The probe is not part of the 19-fixture corpus. A missing hinted file is
    reported as ``unavailable`` and does not fail corpus validation. An
    explicitly supplied path that is missing, or a hash mismatch for an
    available file, is a hard error.
    """
    probes = [f for f in manifest["fixtures"] if not f["corpus_member"]]
    results: list[dict[str, Any]] = []
    for probe in probes:
        hint = probe.get("input_path_hint")
        path: Path | None = None
        source = "unavailable"
        if explicit_path is not None:
            path = explicit_path
            source = "explicit"
        elif hint:
            path = expand_path_hint(hint)
            source = "manifest hint"
        expected = probe["input_sha256"]
        if path is None:
            results.append(
                {
                    "stem": probe["stem"],
                    "status": "unavailable",
                    "source": source,
                    "expected_sha256": expected,
                    "actual_sha256": None,
                    "note": "no input path hint or explicit probe path supplied",
                }
            )
            continue
        if not path.is_file():
            if explicit_path is not None:
                raise MetricsError(f"separate probe input not found: {path}")
            results.append(
                {
                    "stem": probe["stem"],
                    "status": "unavailable",
                    "source": source,
                    "expected_sha256": expected,
                    "actual_sha256": None,
                    "note": "hinted probe input is not present; not regenerated",
                }
            )
            continue
        actual = sha256_file(path)
        if actual != expected:
            raise MetricsError(
                "separate probe input SHA-256 mismatch\n"
                f"  stem:     {probe['stem']}\n"
                f"  path:     {path}\n"
                f"  expected: {expected}\n"
                f"  actual:   {actual}\n"
                "The probe is not part of the 19-fixture corpus and is never "
                "regenerated or substituted."
            )
        results.append(
            {
                "stem": probe["stem"],
                "status": "matched",
                "source": source,
                "expected_sha256": expected,
                "actual_sha256": actual,
                "note": "declared separate-probe hash verified; probe not measured",
            }
        )
    return results


def rust_metadata(repo_root: Path, baseline: dict[str, Any]) -> dict[str, Any]:
    baseline_commit = baseline["rust_source_commit"]
    commit = run_git_stdout(repo_root, ["rev-parse", "HEAD"], role="Rust").strip()
    status = run_git_stdout(
        repo_root,
        [
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            "rust/src",
            "rust/Cargo.toml",
            "rust/Cargo.lock",
        ],
        role="Rust",
    )
    rustc = run_command(["rustc", "--version"], timeout=30).stdout.strip()
    cargo = run_command(["cargo", "--version"], timeout=30).stdout.strip()
    dirty = bool(status.strip())
    declared_toolchain = str(baseline.get("rust_toolchain", ""))
    toolchain_match = toolchain_matches_declared(declared_toolchain, rustc, cargo)
    current_run_verified = (
        commit == baseline_commit
        and not dirty
        and toolchain_match is not False
    )
    return {
        "source_commit": commit,
        "baseline_source_commit": baseline_commit,
        "source_matches_baseline": commit == baseline_commit,
        "sources_dirty": dirty,
        "dirty_paths": [line[3:] for line in status.splitlines() if line.strip()],
        "relevant_paths": ["rust/src", "rust/Cargo.toml", "rust/Cargo.lock"],
        "rustc_version": rustc,
        "cargo_version": cargo,
        "declared_toolchain": declared_toolchain,
        "toolchain_matches_declared": toolchain_match,
        "build_environment": {
            key: os.environ.get(key) for key in RUST_BUILD_ENV_KEYS
        },
        "build_environment_source": "measured process environment",
        "build_profile": baseline.get("rust_build_profile", "test/dev"),
        "build_profile_source": "manifest declaration",
        "build_command": baseline.get(
            "rust_build_command",
            "cargo test --features aac_enc m58_corpus_validation -- --nocapture",
        ),
        "build_command_source": "manifest declaration",
        "current_run_source_verified": current_run_verified,
    }


def find_rust_adts(directory: Path, stem: str) -> Path:
    candidates = [
        directory / f"{stem}.m58.aac",
        directory / f"{stem}.rust.aac",
        directory / f"{stem}.aac",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise MetricsError(
        f"missing Rust ADTS for {stem} in {directory}; expected one of: "
        + ", ".join(c.name for c in candidates)
    )


def collect_rust_adts(
    fixtures: list[dict[str, Any]],
    *,
    repo_root: Path,
    work_dir: Path,
    rust_adts_dir: Path | None,
) -> tuple[dict[str, Path], dict[str, Any]]:
    output_dir = work_dir / "rust-adts"
    output_dir.mkdir(parents=True, exist_ok=True)
    paths: dict[str, Path] = {}
    harness_ran = False

    if rust_adts_dir is not None:
        if not rust_adts_dir.is_dir():
            raise MetricsError(f"--rust-adts-dir is not a directory: {rust_adts_dir}")
        source_dir = rust_adts_dir
    else:
        harness_ran = True
        cp = run_command(
            [
                "cargo",
                "test",
                "--features",
                "aac_enc",
                "m58_corpus_validation",
                "--",
                "--nocapture",
            ],
            cwd=repo_root / "rust",
            timeout=600,
            allow_stderr=True,
        )
        (work_dir / "rust-harness.log").write_text(cp.stdout + cp.stderr, encoding="utf-8")
        source_dir = Path(tempfile.gettempdir()) / "m52-rust-aac" / "m58-corpus"
        if not source_dir.is_dir():
            raise MetricsError(
                "Rust harness ran but did not create its output directory: "
                f"{source_dir}"
            )

    for fixture in fixtures:
        stem = fixture["stem"]
        source = find_rust_adts(source_dir, stem)
        destination = output_dir / f"{stem}.m58.aac"
        if source.resolve() != destination.resolve():
            shutil.copyfile(source, destination)
        paths[stem] = destination

    return paths, {
        "adts_source": "pre-generated" if rust_adts_dir is not None else "cargo harness",
        "harness_ran": harness_ran,
        "adts_source_directory_label": (
            "pre-generated (--rust-adts-dir)"
            if rust_adts_dir is not None
            else "$TMPDIR/m52-rust-aac/m58-corpus"
        ),
    }


def find_c_adts(directory: Path, stem: str) -> Path:
    candidates = [
        directory / f"{stem}.c.aac",
        directory / f"{stem}.aac",
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    raise MetricsError(
        f"missing C ADTS for {stem} in {directory}; expected one of: "
        + ", ".join(c.name for c in candidates)
    )


def _public_link_name(value: str) -> str:
    """Keep stable link identities; hide absolute non-system build paths."""
    if value.startswith("/") and not value.startswith(("/usr/lib/", "/System/")):
        return f"<absolute>/{Path(value).name}"
    return value


def inspect_macho(path: Path) -> dict[str, Any]:
    """Best-effort measured Mach-O metadata.

    Returns an ``unavailable`` status rather than failing when the platform or
    tooling cannot inspect the binary. ``rpaths`` is kept for library lookup
    but is not written to JSON because build rpaths are machine-specific.
    """
    if sys.platform != "darwin":
        return {"status": "unavailable", "reason": "platform is not macOS"}
    otool = shutil.which("otool")
    if otool is None:
        return {"status": "unavailable", "reason": "otool is not available"}
    try:
        linked = run_command([otool, "-L", str(path)], timeout=30)
        load_commands = run_command([otool, "-l", str(path)], timeout=30)
    except MetricsError as exc:
        return {"status": "unavailable", "reason": str(exc)}

    libraries = []
    for line in linked.stdout.splitlines()[1:]:
        stripped = line.strip()
        if not stripped:
            continue
        libraries.append(
            _public_link_name(stripped.split(" (compatibility")[0].strip())
        )

    rpaths: list[str] = []
    lines = load_commands.stdout.splitlines()
    for index, line in enumerate(lines):
        if line.strip() != "cmd LC_RPATH":
            continue
        for follow in lines[index + 1 : index + 4]:
            stripped = follow.strip()
            if stripped.startswith("path "):
                rpaths.append(stripped[5:].split(" (offset")[0].strip())
                break

    return {
        "status": "measured",
        "tool": "otool",
        "linked_libraries": libraries,
        "rpath_count": len(rpaths),
        "rpaths": rpaths,
    }


def c_binary_metadata(
    path: Path,
    expected_sha: str,
    role: str,
    *,
    allow_different_reference: bool,
) -> dict[str, Any]:
    if not path.is_file() or not os.access(path, os.X_OK):
        raise MetricsError(f"missing or non-executable {role} binary: {path}")
    actual = sha256_file(path)
    match = actual == expected_sha
    if not match and not allow_different_reference:
        raise MetricsError(
            f"{role} binary does not match the recorded V3 reference\n"
            f"  path:     {path}\n"
            f"  expected: {expected_sha}\n"
            f"  actual:   {actual}\n"
            "Point --c-enc-bin/--c-dec-bin at the exact retained V3 binaries, or pass\n"
            "--allow-different-reference to run a clearly labelled exploratory "
            "measurement (all results become non-gated)."
        )
    inspection = inspect_macho(path)
    inspection.pop("rpaths", None)
    return {
        "file_name": path.name,
        "sha256": actual,
        "expected_sha256": expected_sha,
        "matches_expected": match,
        "role": role,
        "binary_inspection": inspection,
    }


def collect_c_adts(
    fixtures: list[dict[str, Any]],
    inputs: dict[str, Path],
    *,
    manifest: dict[str, Any],
    work_dir: Path,
    c_enc_bin: Path | None,
    c_adts_dir: Path | None,
    allow_different_reference: bool,
) -> tuple[dict[str, Path], dict[str, Any]]:
    expected = manifest["baseline"]["expected_c_binaries"]["aac-enc"]["sha256"]
    output_dir = work_dir / "c-adts"
    output_dir.mkdir(parents=True, exist_ok=True)
    paths: dict[str, Path] = {}

    encoder_meta: dict[str, Any] | None = None
    encoder_ran = False
    if c_enc_bin is not None:
        encoder_meta = c_binary_metadata(
            c_enc_bin,
            expected,
            "C encoder",
            allow_different_reference=allow_different_reference,
        )
    if c_adts_dir is not None:
        if not c_adts_dir.is_dir():
            raise MetricsError(f"--c-adts-dir is not a directory: {c_adts_dir}")
        source_dir = c_adts_dir
    else:
        if c_enc_bin is None:
            raise MetricsError(
                "no C encoder was supplied. The V3 baseline requires the exact "
                f"f14ad40 Release aac-enc (SHA-256 {expected}). Build it with:\n"
                "  cmake -S <reference> -B <build> -DBUILD_PROGRAMS=ON "
                "-DCMAKE_BUILD_TYPE=Release\n"
                "  cmake --build <build> --target aac-enc aac-dec -j 8\n"
                "or pass --c-adts-dir with retained V3 C ADTS (gated only where "
                "each file matches --v3-reference-dir)."
            )
        encoder_ran = True
        for fixture in fixtures:
            stem = fixture["stem"]
            destination = output_dir / f"{stem}.c.aac"
            cp = run_command(
                [str(c_enc_bin), str(inputs[stem]), str(destination)],
                timeout=120,
                allow_stderr=False,
            )
            if cp.stdout.strip():
                # The reference CLI should be quiet; retain the text in the log.
                (work_dir / "c-encode-stdout.log").write_text(
                    cp.stdout, encoding="utf-8"
                )
            paths[stem] = destination
        source_dir = output_dir

    for fixture in fixtures:
        stem = fixture["stem"]
        if stem not in paths:
            source = find_c_adts(source_dir, stem)
            destination = output_dir / f"{stem}.c.aac"
            if source.resolve() != destination.resolve():
                shutil.copyfile(source, destination)
            paths[stem] = destination

    return paths, {
        "encoder_binary": encoder_meta,
        "encoder_ran": encoder_ran,
        "adts_source": "C encoder" if encoder_ran else "pre-generated",
    }


def count_clipped_samples(pcm: list[list[int]]) -> int:
    return sum(1 for channel in pcm for value in channel if abs(value) >= 32767)


def decode_stream(
    decoder: Path,
    adts: Path,
    wav_path: Path,
    *,
    expected_channels: int,
    expected_sample_rate: int,
) -> dict[str, Any]:
    cp = run_command(
        [str(decoder), str(adts), str(wav_path)],
        timeout=120,
        allow_stderr=False,
    )
    if cp.stdout.strip():
        # aac-dec is normally silent; keep any unexpected text as a failure.
        raise MetricsError(
            f"decoder wrote to stdout for {adts.name}: {cp.stdout[-2000:]}"
        )
    wav = read_wav_i16(wav_path)
    if wav["channels"] != expected_channels:
        raise MetricsError(
            f"{adts.name}: decoded channels {wav['channels']} != {expected_channels}"
        )
    if wav["sample_rate_hz"] != expected_sample_rate:
        raise MetricsError(
            f"{adts.name}: decoded rate {wav['sample_rate_hz']} != {expected_sample_rate}"
        )
    return {
        "decode_exit_code": cp.returncode,
        "decode_stderr_empty": cp.stderr == "",
        "wav_sha256": sha256_file(wav_path),
        "sample_rate_hz": wav["sample_rate_hz"],
        "channels": wav["channels"],
        "samples_per_channel": wav["samples_per_channel"],
        "bits_per_sample": wav["bits_per_sample"],
        "clipped_samples": count_clipped_samples(wav["pcm"]),
        "wav": wav,
    }


def c_source_metadata(
    c_ref_dir: Path | None,
    expected_commit: str,
    *,
    allow_different_reference: bool,
) -> dict[str, Any]:
    if c_ref_dir is None:
        return {
            "status": "not supplied",
            "commit": None,
            "matches_baseline": None,
            "clean": None,
            "dirty_paths": [],
            "untracked_files_checked": False,
            "note": "source identity not verified; pass --c-ref-dir to verify it",
        }
    if not c_ref_dir.is_dir():
        raise MetricsError(f"--c-ref-dir is not a directory: {c_ref_dir}")
    commit = run_git_stdout(c_ref_dir, ["rev-parse", "HEAD"], role="C reference").strip()
    status = run_git_stdout(
        c_ref_dir,
        ["status", "--porcelain", "--untracked-files=no"],
        role="C reference",
    )
    dirty_paths = [line[3:] for line in status.splitlines() if line.strip()]
    matches = commit == expected_commit
    if not matches and not allow_different_reference:
        raise MetricsError(
            "C reference checkout does not match the recorded V3 source\n"
            f"  path:     {c_ref_dir}\n"
            f"  expected: {expected_commit}\n"
            f"  actual:   {commit}\n"
            "Check out the pinned f14ad40 revision, or pass "
            "--allow-different-reference for a clearly labelled exploratory run."
        )
    return {
        "status": (
            "verified"
            if matches and not dirty_paths
            else "mismatch (override)"
            if not matches
            else "verified (dirty tracked files)"
        ),
        "commit": commit,
        "matches_baseline": matches,
        "clean": not dirty_paths,
        "dirty_paths": dirty_paths,
        "untracked_files_checked": False,
        "note": "tracked modifications only; build/untracked files do not change HEAD identity",
    }


def resolve_c_library(
    c_lib: Path | None,
    c_enc_bin: Path | None,
    c_dec_bin: Path,
    decoder_inspection: dict[str, Any],
) -> tuple[Path | None, dict[str, Any]]:
    if c_lib is not None:
        if not c_lib.is_file():
            raise MetricsError(f"C library not found: {c_lib}")
        return c_lib, {"status": "found", "method": "explicit"}

    candidates: list[tuple[Path, str]] = []

    linked_libraries = decoder_inspection.get("linked_libraries", [])
    rpaths = decoder_inspection.get("rpaths", [])
    for linked in linked_libraries:
        if "libfdk-aac" not in linked:
            continue
        name = Path(linked).name
        if linked.startswith("/"):
            candidate = Path(linked)
            if candidate.is_file():
                candidates.append((candidate, "absolute-linked-path"))
        for rpath in rpaths:
            candidate = Path(rpath) / name
            if candidate.is_file():
                candidates.append((candidate, "rpath"))

    for binary in (c_dec_bin, c_enc_bin):
        if binary is None:
            continue
        for name in ("libfdk-aac.2.0.4.dylib", "libfdk-aac.2.dylib"):
            candidate = binary.parent / name
            if candidate.is_file():
                candidates.append((candidate, "sibling"))

    unique: list[tuple[Path, str]] = []
    seen: set[str] = set()
    for candidate, method in candidates:
        key = str(candidate.resolve())
        if key in seen:
            continue
        seen.add(key)
        unique.append((candidate, method))
    if not unique:
        return None, {
            "status": "unavailable",
            "reason": "no sibling, absolute, or rpath-resolved libfdk-aac library",
        }
    return unique[0][0], {"status": "found", "method": unique[0][1]}


def c_reference_metadata(
    manifest: dict[str, Any],
    *,
    c_enc_bin: Path | None,
    c_dec_bin: Path,
    c_ref_dir: Path | None,
    c_lib: Path | None,
    allow_different_reference: bool,
) -> dict[str, Any]:
    expected = manifest["baseline"]["expected_c_binaries"]
    encoder_meta = None
    if c_enc_bin is not None:
        encoder_meta = c_binary_metadata(
            c_enc_bin,
            expected["aac-enc"]["sha256"],
            "C encoder",
            allow_different_reference=allow_different_reference,
        )
    decoder_meta = c_binary_metadata(
        c_dec_bin,
        expected["aac-dec"]["sha256"],
        "C decoder",
        allow_different_reference=allow_different_reference,
    )

    source = c_source_metadata(
        c_ref_dir,
        manifest["baseline"]["c_source_commit"],
        allow_different_reference=allow_different_reference,
    )
    decoder_inspection = inspect_macho(c_dec_bin)
    library_path, library_lookup = resolve_c_library(
        c_lib, c_enc_bin, c_dec_bin, decoder_inspection
    )
    library_meta = None
    library_matches = None
    if library_path is not None:
        actual_lib = sha256_file(library_path)
        expected_lib = expected["libfdk-aac.2.0.4.dylib"]["sha256"]
        library_matches = actual_lib == expected_lib
        if not library_matches and not allow_different_reference:
            raise MetricsError(
                "C shared library does not match the recorded V3 reference\n"
                f"  path:     {library_path}\n"
                f"  expected: {expected_lib}\n"
                f"  actual:   {actual_lib}\n"
                "Use the retained ref-build library, or pass "
                "--allow-different-reference for a clearly labelled exploratory run."
            )
        library_meta = {
            "file_name": library_path.name,
            "sha256": actual_lib,
            "expected_sha256": expected_lib,
            "matches_expected": library_matches,
            "role": "FDK AAC shared library",
        }

    encoder_exact = bool(encoder_meta and encoder_meta["matches_expected"])
    decoder_exact = bool(decoder_meta["matches_expected"])
    source_ok = source["matches_baseline"] is not False and source["clean"] is not False
    library_ok = library_matches is not False
    exact = (
        encoder_exact
        and decoder_exact
        and source_ok
        and library_ok
        and not allow_different_reference
    )
    return {
        "source_commit": source["commit"],
        "source_matches_baseline": source["matches_baseline"],
        "source_clean": source["clean"],
        "source_dirty_paths": source["dirty_paths"],
        "source_verification": {
            "status": source["status"],
            "note": source["note"],
            "untracked_files_checked": source["untracked_files_checked"],
        },
        "encoder_binary": encoder_meta,
        "decoder_binary": decoder_meta,
        "encoder_exact": encoder_exact,
        "decoder_exact": decoder_exact,
        "library": library_meta,
        "library_lookup": library_lookup,
        "library_matches_expected": library_matches,
        "reference_exact": exact,
        "reference_exact_definition": (
            "encoder and decoder hashes exact; supplied source is at the "
            "recorded revision with no tracked modifications; supplied "
            "library hash not mismatched; no --allow-different-reference override"
        ),
        "allow_different_reference": allow_different_reference,
        "build_metadata": {
            "source": "manifest declaration (not recovered from binary)",
            "build_type": manifest["baseline"]["c_build_type"],
            "cmake_options": manifest["baseline"]["c_cmake_options"],
            "compiler": manifest["baseline"]["c_compiler"],
            "cmake_version": manifest["baseline"]["cmake"],
        },
        "build_type": manifest["baseline"]["c_build_type"],
        "cmake_options": manifest["baseline"]["c_cmake_options"],
        "compiler": manifest["baseline"]["c_compiler"],
        "cmake_version": manifest["baseline"]["cmake"],
    }


def artifact_provenance(
    stem: str,
    *,
    rust_meta: dict[str, Any],
    rust_collection: dict[str, Any],
    c_meta: dict[str, Any],
    c_collection: dict[str, Any],
    v3_item: dict[str, Any] | None,
    allow_different_reference: bool,
) -> dict[str, Any]:
    """Per-fixture stream provenance.

    A stream is verified only when it was produced by a verified encoder /
    harness in this invocation, or when the exact ADTS file matches the
    retained V3 artifact. A supplied directory or an unexecuted binary hash is
    never sufficient on its own.
    """
    v3_rust_match = bool(v3_item and v3_item.get("rust_adts") is True)
    v3_c_match = bool(v3_item and v3_item.get("c_adts") is True)

    rust_harness_ran = bool(rust_collection.get("harness_ran"))
    rust_source_ok = bool(rust_meta.get("source_matches_baseline"))
    rust_tree_clean = not bool(rust_meta.get("sources_dirty"))
    rust_toolchain_ok = rust_meta.get("toolchain_matches_declared") is not False
    rust_current = (
        rust_harness_ran and rust_source_ok and rust_tree_clean and rust_toolchain_ok
    )

    c_encoder_ran = bool(c_collection.get("encoder_ran"))
    c_current = bool(
        c_encoder_ran
        and c_meta.get("encoder_exact")
        and c_meta.get("decoder_exact")
        and c_meta.get("source_matches_baseline") is not False
        and c_meta.get("source_clean") is not False
        and c_meta.get("library_matches_expected") is not False
    )

    if allow_different_reference:
        rust_verified = False
        rust_reason = (
            "explicit --allow-different-reference override forces exploratory "
            "classification"
        )
        c_verified = False
        c_reason = rust_reason
    else:
        rust_verified = rust_current or v3_rust_match
        if rust_current:
            rust_reason = (
                "Rust harness ran in this invocation at the recorded baseline "
                "commit with a clean relevant tree"
            )
            if rust_meta.get("toolchain_matches_declared") is None:
                rust_reason += " (declared toolchain version could not be parsed)"
        elif v3_rust_match:
            rust_reason = "Rust ADTS matches the retained V3 artifact for this fixture"
        else:
            if not rust_harness_ran:
                rust_reason = (
                    "pre-generated Rust ADTS without a retained V3 per-file match"
                )
            else:
                parts = ["Rust harness ran but"]
                if not rust_source_ok:
                    parts.append(
                        f"source {rust_meta.get('source_commit')} is not the "
                        f"baseline {rust_meta.get('baseline_source_commit')}"
                    )
                if not rust_tree_clean:
                    parts.append("the relevant Rust tree is dirty")
                if not rust_toolchain_ok:
                    parts.append("the measured toolchain does not match the declaration")
                parts.append("and no retained V3 per-file match was supplied")
                rust_reason = "; ".join(parts)

        c_verified = bool(c_current or (v3_c_match and c_meta.get("decoder_exact")))
        if c_current:
            c_reason = (
                "C encoder ran in this invocation with exact encoder/decoder "
                "hashes and non-mismatched source/library metadata"
            )
        elif v3_c_match and c_meta.get("decoder_exact"):
            c_reason = "C ADTS matches the retained V3 artifact for this fixture"
        else:
            if not c_encoder_ran:
                c_reason = (
                    "pre-generated C ADTS without a retained V3 per-file match; "
                    "an encoder binary hash alone is not proof that this file came "
                    "from that encoder"
                )
            else:
                parts = ["C encoder ran but"]
                if not c_meta.get("encoder_exact"):
                    parts.append("the encoder binary hash is not exact")
                if not c_meta.get("decoder_exact"):
                    parts.append("the decoder binary hash is not exact")
                if c_meta.get("source_matches_baseline") is False:
                    parts.append("the C source revision is not the baseline")
                if c_meta.get("source_clean") is False:
                    parts.append("the C source tree is dirty")
                if c_meta.get("library_matches_expected") is False:
                    parts.append("the FDK library hash is not exact")
                parts.append("and no retained V3 per-file match was supplied")
                c_reason = "; ".join(parts)

    return {
        "stem": stem,
        "override": allow_different_reference,
        "rust": {
            "verified": bool(rust_verified),
            "current_run_source_verified": rust_current and not allow_different_reference,
            "retained_v3_adts_match": v3_rust_match,
            "reason": rust_reason,
        },
        "c": {
            "verified": bool(c_verified),
            "current_run_encoder_verified": c_current and not allow_different_reference,
            "retained_v3_adts_match": v3_c_match,
            "reason": c_reason,
        },
        "both_verified": bool(rust_verified and c_verified),
    }


def decoded_length_check(
    *,
    frame_count: int,
    decoded_samples_per_channel: int,
    sample_rate_hz: int,
    output_delays: dict[str, Any],
    frame_length_samples: int,
) -> dict[str, Any]:
    """Compare decoded length with frame_count*frame_length - outputDelay.

    The retained V3 FDK decoder CLI satisfies this identity exactly for all
    38 corpus streams. The check is exact by design; an unknown sample rate is
    reported as ``unavailable`` rather than given an invented tolerance.
    """
    delay = output_delays.get(str(sample_rate_hz))
    if delay is None:
        return {
            "status": "unavailable",
            "plausible": None,
            "frame_count": frame_count,
            "frame_length_samples": frame_length_samples,
            "output_delay_samples": None,
            "expected_samples_per_channel": None,
            "actual_samples_per_channel": decoded_samples_per_channel,
            "delta_samples": None,
            "reason": f"no decoded output-delay recorded for {sample_rate_hz} Hz",
        }
    expected = frame_count * frame_length_samples - int(delay)
    delta = decoded_samples_per_channel - expected
    return {
        "status": "plausible" if delta == 0 else "implausible",
        "plausible": delta == 0,
        "frame_count": frame_count,
        "frame_length_samples": frame_length_samples,
        "output_delay_samples": int(delay),
        "expected_samples_per_channel": expected,
        "actual_samples_per_channel": decoded_samples_per_channel,
        "delta_samples": delta,
        "reason": (
            "decoded length equals frame_count*frame_length - outputDelay"
            if delta == 0
            else "decoded length differs from frame_count*frame_length - outputDelay"
        ),
    }


def decoded_length_state(checks: dict[str, dict[str, Any]]) -> str:
    values = [check.get("plausible") for check in checks.values()]
    if any(value is False for value in values):
        return "implausible"
    if all(value is True for value in values):
        return "verified"
    return "unavailable"


def decide_scope(
    *,
    static_gate: bool,
    alignment_ok: bool,
    stream_provenance_ok: bool,
    decoded_length_state_value: str,
) -> str:
    if not alignment_ok or decoded_length_state_value == "implausible":
        return "excluded"
    if (
        not static_gate
        or not stream_provenance_ok
        or decoded_length_state_value != "verified"
    ):
        return "exploratory"
    return "gated"


def baseline_reproduction_status(
    fixtures: list[dict[str, Any]],
    v3_comparison: dict[str, Any] | None,
) -> dict[str, Any]:
    expected = len(fixtures)
    full_corpus = expected == 19
    if v3_comparison is None:
        return {
            "baseline_reproduced": None,
            "baseline_reproduced_for_selection": None,
            "scope": None,
            "selected_fixture_count": expected,
            "full_corpus_selected": full_corpus,
            "all_selected_artifacts_present": None,
            "all_selected_artifacts_matched": None,
            "note": "not checked (--v3-reference-dir not supplied)",
        }
    summary = v3_comparison["summary"]
    present = all(int(value["compared"]) == expected for value in summary.values())
    matched = present and all(
        int(value["matched"]) == expected for value in summary.values()
    )
    if full_corpus:
        note = (
            "all 19 corpus inputs and both implementations' ADTS/decoded WAVs "
            "match the retained V3 artifacts byte for byte"
            if matched
            else "full 19-fixture corpus selected, but at least one artifact "
            "class is incomplete or mismatched"
        )
    else:
        note = (
            "all selected artifacts match; this does not establish full-corpus "
            "reproduction"
            if matched
            else "at least one selected artifact class is incomplete or mismatched"
        )
    return {
        "baseline_reproduced": bool(matched) if full_corpus else None,
        "baseline_reproduced_for_selection": bool(matched),
        "scope": "full-corpus" if full_corpus else "selection",
        "selected_fixture_count": expected,
        "full_corpus_selected": full_corpus,
        "all_selected_artifacts_present": present,
        "all_selected_artifacts_matched": matched,
        "note": note,
    }


def compare_v3(
    fixtures: list[dict[str, Any]],
    *,
    v3_dir: Path | None,
    inputs: dict[str, Path],
    rust_adts: dict[str, Path],
    c_adts: dict[str, Path],
    decoded: dict[str, dict[str, Path]],
) -> dict[str, Any] | None:
    if v3_dir is None:
        return None
    if not v3_dir.is_dir():
        raise MetricsError(f"--v3-reference-dir is not a directory: {v3_dir}")
    per_fixture: dict[str, Any] = {}
    counts = {
        "input": [0, 0],
        "rust_adts": [0, 0],
        "c_adts": [0, 0],
        "rust_decoded_wav": [0, 0],
        "c_decoded_wav": [0, 0],
    }

    def check(key: str, actual: str, expected_path: Path) -> bool | None:
        if not expected_path.is_file():
            return None
        expected = sha256_file(expected_path)
        match = actual == expected
        counts[key][1] += 1
        if match:
            counts[key][0] += 1
        return match

    for fixture in fixtures:
        stem = fixture["stem"]
        item: dict[str, Any] = {}
        item["input"] = check("input", sha256_file(inputs[stem]), v3_dir / "inputs" / f"{stem}.wav")
        item["rust_adts"] = check(
            "rust_adts", sha256_file(rust_adts[stem]), v3_dir / "rust-adts" / f"{stem}.rust.aac"
        )
        item["c_adts"] = check(
            "c_adts", sha256_file(c_adts[stem]), v3_dir / "c-adts" / f"{stem}.c.aac"
        )
        item["rust_decoded_wav"] = check(
            "rust_decoded_wav",
            sha256_file(decoded[stem]["rust"]),
            v3_dir / "rust-wav" / f"{stem}.rust.wav",
        )
        item["c_decoded_wav"] = check(
            "c_decoded_wav",
            sha256_file(decoded[stem]["c"]),
            v3_dir / "c-wav" / f"{stem}.c.wav",
        )
        per_fixture[stem] = item

    summary = {
        key: {
            "matched": value[0],
            "compared": value[1],
            "expected": len(fixtures),
        }
        for key, value in counts.items()
    }
    all_compared = all(value["compared"] > 0 for value in summary.values())
    complete = all(value["compared"] == len(fixtures) for value in summary.values())
    all_matched = complete and all(
        value["matched"] == len(fixtures) for value in summary.values()
    )
    return {
        "reference_label": "retained-v3",
        "summary": summary,
        "all_compared": all_compared,
        "selection_complete": complete,
        "all_matched": all_matched,
        "per_fixture": per_fixture,
    }


def descriptive(values: list[float]) -> dict[str, Any]:
    clean = [v for v in values if v is not None and math.isfinite(v)]
    if not clean:
        return {"count": 0, "min": None, "max": None, "median": None}
    return {
        "count": len(clean),
        "min": round(min(clean), 6),
        "max": round(max(clean), 6),
        "median": round(float(statistics.median(clean)), 6),
    }


def summary_for_scope(results: list[dict[str, Any]], scope: str) -> dict[str, Any]:
    items = [r for r in results if r["eligibility"]["scope"] == scope]
    snr_agreement: list[float] = []
    bitrate_delta: list[float] = []
    residue: list[float] = []
    frame_equal = 0
    length_equal = 0
    residue_identical = 0
    provenance_verified = 0
    decoded_length_verified = 0
    for item in items:
        if item["eligibility"]["stream_provenance_verified"]:
            provenance_verified += 1
        if item["eligibility"]["decoded_length_status"] == "verified":
            decoded_length_verified += 1
        quality = item.get("quality")
        if quality is not None:
            r_snr = quality["rust_vs_original"]["aggregate"]["snr_db"]
            c_snr = quality["c_vs_original"]["aggregate"]["snr_db"]
            if r_snr is not None and c_snr is not None:
                snr_agreement.append(abs(r_snr - c_snr))
            rc = quality.get("rust_vs_c")
            if rc is not None:
                if rc["aggregate"]["snr_db"] is not None:
                    residue.append(rc["aggregate"]["snr_db"])
                elif rc["aggregate"]["snr_status"] == "identical":
                    residue_identical += 1
        r_bytes = item["encoded"]["rust"]["total_bytes"]
        c_bytes = item["encoded"]["c"]["total_bytes"]
        if c_bytes:
            bitrate_delta.append((r_bytes - c_bytes) / c_bytes * 100.0)
        if item["encoded"]["rust"]["frame_count"] == item["encoded"]["c"]["frame_count"]:
            frame_equal += 1
        if (
            item["decoded"]["rust"]["samples_per_channel"]
            == item["decoded"]["c"]["samples_per_channel"]
        ):
            length_equal += 1
    return {
        "count": len(items),
        "stems": [item["stem"] for item in items],
        "snr_agreement_abs_db": descriptive(snr_agreement),
        "bitrate_delta_percent": descriptive(bitrate_delta),
        "residue_snr_db": descriptive(residue),
        "residue_identical_count": residue_identical,
        "frame_count_equal_count": frame_equal,
        "decoded_length_equal_count": length_equal,
        "stream_provenance_verified_count": provenance_verified,
        "decoded_length_verified_count": decoded_length_verified,
    }


def build_result(
    fixture: dict[str, Any],
    *,
    input_path: Path,
    rust_adts_path: Path,
    c_adts_path: Path,
    decoded_meta: dict[str, dict[str, Any]],
    alignment_contract: dict[str, Any],
    provenance: dict[str, Any],
    decoder_output_delays: dict[str, Any],
    frame_length_samples: int,
    v3_item: dict[str, Any] | None,
) -> dict[str, Any]:
    stem = fixture["stem"]
    original = read_wav_i16(input_path)
    if original["channels"] != fixture["channels"]:
        raise MetricsError(f"{stem}: input channels do not match manifest")
    if original["sample_rate_hz"] != fixture["sample_rate_hz"]:
        raise MetricsError(f"{stem}: input sample rate does not match manifest")
    if original["samples_per_channel"] != fixture["samples_per_channel"]:
        raise MetricsError(f"{stem}: input sample count does not match manifest")

    duration = float(fixture["duration_seconds"])
    encoded = {
        "rust": parse_adts(rust_adts_path),
        "c": parse_adts(c_adts_path),
    }
    for impl in ("rust", "c"):
        encoded[impl]["bitrate_kbps_total"] = round(
            encoded[impl]["total_bytes"] * 8 / duration / 1000.0, 6
        )
        encoded[impl]["bitrate_kbps_payload"] = round(
            encoded[impl]["payload_bytes"] * 8 / duration / 1000.0, 6
        )

    decoded = {}
    length_checks: dict[str, dict[str, Any]] = {}
    for impl in ("rust", "c"):
        meta = decoded_meta[impl]
        wav = meta["wav"]
        if wav["channels"] != fixture["channels"]:
            raise MetricsError(f"{stem}/{impl}: decoded channels do not match manifest")
        if wav["sample_rate_hz"] != fixture["sample_rate_hz"]:
            raise MetricsError(f"{stem}/{impl}: decoded sample rate does not match manifest")
        length_check = decoded_length_check(
            frame_count=encoded[impl]["frame_count"],
            decoded_samples_per_channel=wav["samples_per_channel"],
            sample_rate_hz=fixture["sample_rate_hz"],
            output_delays=decoder_output_delays,
            frame_length_samples=frame_length_samples,
        )
        length_checks[impl] = length_check
        decoded[impl] = {
            "decode_exit_code": meta["decode_exit_code"],
            "decode_stderr_empty": meta["decode_stderr_empty"],
            "wav_sha256": meta["wav_sha256"],
            "sample_rate_hz": wav["sample_rate_hz"],
            "channels": wav["channels"],
            "samples_per_channel": wav["samples_per_channel"],
            "bits_per_sample": wav["bits_per_sample"],
            "clipped_samples": meta["clipped_samples"],
            "length_check": length_check,
            "wav": wav,
        }
    length_state = decoded_length_state(length_checks)

    alignment = {
        "rust": estimate_alignment(original, decoded["rust"]["wav"], alignment_contract),
        "c": estimate_alignment(original, decoded["c"]["wav"], alignment_contract),
    }
    alignment_ok = (
        not alignment["rust"]["ambiguous"]
        and not alignment["c"]["ambiguous"]
        and alignment["rust"]["selected_lag_samples"] is not None
        and alignment["c"]["selected_lag_samples"] is not None
    )

    quality = None
    if alignment_ok and length_state != "implausible":
        quality = {
            "rust_vs_original": measure_pair(
                original,
                decoded["rust"]["wav"],
                lag_samples=alignment["rust"]["selected_lag_samples"],
                reference_name="original",
                test_name="rust_decode",
            ),
            "c_vs_original": measure_pair(
                original,
                decoded["c"]["wav"],
                lag_samples=alignment["c"]["selected_lag_samples"],
                reference_name="original",
                test_name="c_decode",
            ),
            "rust_vs_c": measure_pair(
                decoded["rust"]["wav"],
                decoded["c"]["wav"],
                lag_samples=0,
                reference_name="rust_decode",
                test_name="c_decode",
            ),
        }

    static_gate = bool(fixture["eligibility"]["gated_aggregate"])
    scope = decide_scope(
        static_gate=static_gate,
        alignment_ok=alignment_ok,
        stream_provenance_ok=bool(provenance["both_verified"]),
        decoded_length_state_value=length_state,
    )

    reasons: list[str] = []
    if not static_gate:
        reasons.append(fixture["eligibility"]["reason"])
    if not provenance["rust"]["verified"]:
        reasons.append(
            "Rust stream provenance not verified: "
            + provenance["rust"]["reason"]
        )
    if not provenance["c"]["verified"]:
        reasons.append(
            "C stream provenance not verified: " + provenance["c"]["reason"]
        )
    if length_state == "implausible":
        for impl in ("rust", "c"):
            check = length_checks[impl]
            if check["plausible"] is False:
                reasons.append(
                    f"{impl} decoded length implausible: "
                    f"{check['actual_samples_per_channel']} samples vs "
                    f"{check['expected_samples_per_channel']} expected "
                    f"(delta {check['delta_samples']})"
                )
    elif length_state == "unavailable":
        reasons.append(
            "decoded-length plausibility unavailable for the fixture sample rate"
        )
    if not alignment_ok:
        reasons.append(
            "alignment excluded: "
            f"rust={alignment['rust']['ambiguity_status']}"
            f" ({alignment['rust']['exclusion_reason'] or 'n/a'}); "
            f"c={alignment['c']['ambiguity_status']}"
            f" ({alignment['c']['exclusion_reason'] or 'n/a'})"
        )
    if scope == "gated":
        reasons.append(
            "gated: 48 kHz; approved/deterministic fixture provenance; verified "
            "Rust and C stream provenance; plausible decoded length; unambiguous "
            "per-channel alignment"
        )

    return {
        "stem": stem,
        "category": fixture["category"],
        "provenance_status": fixture["provenance_status"],
        "provenance_source": fixture["provenance_source"],
        "license": fixture["license"],
        "corpus_member": fixture["corpus_member"],
        "input": {
            "sha256": fixture["input_sha256"],
            "source": fixture["input_source"],
            "sample_rate_hz": fixture["sample_rate_hz"],
            "channels": fixture["channels"],
            "samples_per_channel": fixture["samples_per_channel"],
            "duration_seconds": fixture["duration_seconds"],
        },
        "eligibility": {
            "static_gated_aggregate": static_gate,
            "gated_aggregate": scope == "gated",
            "scope": scope,
            "static_reason": fixture["eligibility"]["reason"],
            "stream_provenance_verified": bool(provenance["both_verified"]),
            "rust_provenance": provenance["rust"],
            "c_provenance": provenance["c"],
            "decoded_length_status": length_state,
            "decoded_length_plausible": length_state == "verified",
            "reasons": reasons,
        },
        "encoded": encoded,
        "decoded": {
            "rust": {k: v for k, v in decoded["rust"].items() if k != "wav"},
            "c": {k: v for k, v in decoded["c"].items() if k != "wav"},
        },
        "alignment": alignment,
        "quality": quality,
        "v3_comparison": v3_item,
    }


def render_report(result: dict[str, Any]) -> str:
    meta = result["run"]
    fixtures = result["fixtures"]

    def fmt_stats(stats: dict[str, Any]) -> str:
        return (
            f"count={stats['count']}, min={stats['min']}, "
            f"max={stats['max']}, median={stats['median']}"
        )

    lines: list[str] = []
    lines.append("# AAC-LC VBR-5 objective metrics")
    lines.append("")
    lines.append(
        "Measurement-only report. No perceptual, ABX, or listening claim. "
        "No quality pass/fail thresholds are applied."
    )
    lines.append("")
    lines.append("## Run identity")
    lines.append("")
    lines.append("| Field | Value |")
    lines.append("|---|---|")
    lines.append(f"| Tool | `{result['tool']['name']} {result['tool']['version']}` |")
    lines.append(f"| Python | `{meta['python_version']}` |")
    lines.append(f"| Manifest SHA-256 | `{result['manifest']['sha256']}` |")
    lines.append(
        f"| Rust source | `{meta['rust']['source_commit']}` "
        f"(baseline `{meta['rust']['baseline_source_commit']}`, "
        f"match={meta['rust']['source_matches_baseline']}, "
        f"relevant tree dirty={meta['rust']['sources_dirty']}) |"
    )
    lines.append(
        f"| Rust toolchain | `{meta['rust']['rustc_version']}`; "
        f"`{meta['rust']['cargo_version']}` "
        f"(declared match={meta['rust']['toolchain_matches_declared']}) |"
    )
    rustflags = meta["rust"].get("build_environment", {}).get("RUSTFLAGS")
    lines.append(
        f"| Rust build environment | `RUSTFLAGS={rustflags if rustflags else 'unset'}`; "
        f"source=`{meta['rust'].get('build_environment_source')}` |"
    )
    lines.append(
        f"| Rust build | `{meta['rust']['build_command']}` "
        f"(source=`{meta['rust'].get('build_command_source')}`) |"
    )
    lines.append(
        f"| Rust ADTS source | `{meta['rust']['adts_source']}` "
        f"(harness ran={meta['rust']['harness_ran']}) |"
    )
    lines.append(
        f"| C source | `{meta['c']['source_commit'] or 'not provided'}` "
        f"(match={meta['c']['source_matches_baseline']}, "
        f"clean={meta['c'].get('source_clean')}) |"
    )
    enc = meta["c"].get("encoder_binary")
    dec = meta["c"].get("decoder_binary")
    lines.append(
        f"| C encoder | `{enc['file_name']}` `{enc['sha256']}` "
        f"(match={enc['matches_expected']}) |"
        if enc
        else "| C encoder | not run (pre-generated C ADTS) |"
    )
    lines.append(
        f"| C decoder | `{dec['file_name']}` `{dec['sha256']}` "
        f"(match={dec['matches_expected']}) |"
    )
    lines.append(
        f"| C ADTS source | `{meta['c']['adts_source']}` "
        f"(encoder ran={meta['c']['encoder_ran']}) |"
    )
    lib = meta["c"].get("library")
    lookup = meta["c"].get("library_lookup", {})
    if lib:
        lines.append(
            f"| C library | `{lib['file_name']}` `{lib['sha256']}` "
            f"(match={lib['matches_expected']}, lookup=`{lookup.get('method')}`) |"
        )
    else:
        lines.append(
            f"| C library | not identified (`{lookup.get('status', 'unavailable')}`) |"
        )
    build_meta = meta["c"].get("build_metadata", {})
    lines.append(
        f"| C build declaration | `{build_meta.get('build_type')}`; "
        f"`{build_meta.get('compiler')}`; CMake `{build_meta.get('cmake_version')}` "
        f"(source=`{build_meta.get('source')}`) |"
    )
    lines.append(
        f"| Reference exact | `{meta['c']['reference_exact']}`; "
        f"allow override `{meta['c']['allow_different_reference']}` |"
    )
    lines.append(
        "| Gated precondition | per-stream Rust and C provenance verified, "
        "plausible decoded length, unambiguous alignment |"
    )
    lines.append(
        "| Encoder configuration | AAC-LC, VBR-5, afterburner on, PNS off, "
        "ADTS, 1024 samples |"
    )
    lines.append("")
    if meta["rust"]["source_matches_baseline"] is False:
        lines.append(
            "The Rust source commit is ahead of the recorded baseline. This is "
            "a behavioural reproduction only if the retained V3 comparison below "
            "matches; it is not a source-identical rebuild."
        )
        lines.append("")

    lines.append("## Stream provenance")
    lines.append("")
    lines.append(
        "`gated` requires verified provenance for both encoded streams. A "
        "supplied ADTS directory or an unexecuted encoder binary hash is not "
        "sufficient. A stream is verified when a verified encoder/harness ran "
        "in this invocation, or when the exact ADTS file matches the retained "
        "V3 artifact. `--allow-different-reference` forces every stream "
        "unverified. The supplied `--v3-reference-dir` is treated as the "
        "trusted baseline; the tool verifies byte equality, not the historical "
        "authenticity of that directory."
    )
    lines.append("")
    lines.append("| Fixture | Rust evidence | C evidence | Both verified |")
    lines.append("|---|---|---|---|")
    for item in fixtures:
        eligibility = item["eligibility"]
        lines.append(
            f"| {item['stem']} | {eligibility['rust_provenance']['reason']} | "
            f"{eligibility['c_provenance']['reason']} | "
            f"{eligibility['stream_provenance_verified']} |"
        )
    lines.append("")

    lines.append("## Eligibility scope")
    lines.append("")
    lines.append(
        "`gated` means 48 kHz + approved/deterministic fixture provenance + "
        "verified Rust and C stream provenance + plausible decoded length + "
        "unambiguous per-channel alignment. It is an evidence scope, not a "
        "quality score. 44.1 kHz fixtures remain exploratory until the "
        "benchmark-plan separate gate is satisfied; m8 remains exploratory "
        "because its provenance is PROVISIONAL."
    )
    lines.append("")
    lines.append("| Scope | Count | Fixtures |")
    lines.append("|---|---:|---|")
    for scope in ("gated", "exploratory", "excluded"):
        items = [f for f in fixtures if f["eligibility"]["scope"] == scope]
        lines.append(
            f"| {scope} | {len(items)} | "
            + (", ".join(f["stem"] for f in items) if items else "—")
            + " |"
        )
    separate = result["separate_probes"]
    lines.append(
        f"| separate (not measured) | {len(separate)} | "
        + (", ".join(f["stem"] for f in separate) if separate else "—")
        + " |"
    )
    lines.append("")
    non_gated = [f for f in fixtures if f["eligibility"]["scope"] != "gated"]
    if non_gated:
        lines.append("### Non-gated fixture reasons")
        lines.append("")
        for item in non_gated:
            reasons = "; ".join(item["eligibility"]["reasons"]) or "no reason recorded"
            lines.append(
                f"- `{item['stem']}` ({item['eligibility']['scope']}): {reasons}"
            )
        lines.append("")
    if separate:
        lines.append("### Separate probe (not measured)")
        lines.append("")
        lines.append(
            "This fixture is identified for a separate mono/44.1 kHz measurement; "
            "it is not part of the 19-fixture V3 corpus and is not counted above. "
            "Its declared input hash is checked when the input is available; the "
            "probe itself is never measured, regenerated, or substituted."
        )
        lines.append("")
        lines.append("| Stem | Rate/ch | Input SHA-256 | Hash status | Status | Reason |")
        lines.append("|---|---:|---|---|---|---|")
        for probe in separate:
            lines.append(
                f"| {probe['stem']} | {probe['sample_rate_hz']}/{probe['channels']} | "
                f"`{probe['input_sha256']}` | {probe.get('input_hash_status', 'unavailable')} | "
                f"not measured | {probe['reason']} |"
            )
        lines.append("")

    lines.append("## Alignment")
    lines.append("")
    lines.append(
        "Positive lag means `decoded[lag:]` aligns with `original[0:]`. "
        "Each channel is estimated independently over the recorded search "
        "range; stereo channels must agree within ±1 sample. Short signals use "
        "stride 1 and refine over ±2 samples; long signals use stride 8 and "
        "refine over ±4 samples, covering the whole coarse cell. No decoder "
        "`outputDelay` is subtracted from the decoded WAV a second time."
    )
    lines.append("")
    lines.append(
        "| Fixture | Rust channel lags | Rust selected | Rust status | "
        "C channel lags | C selected | C status |"
    )
    lines.append("|---|---:|---:|---|---:|---:|---|")
    for item in fixtures:
        a = item["alignment"]
        r_lags = "/".join(
            "n/a" if ch["estimated_lag_samples"] is None else str(ch["estimated_lag_samples"])
            for ch in a["rust"]["per_channel"]
        )
        c_lags = "/".join(
            "n/a" if ch["estimated_lag_samples"] is None else str(ch["estimated_lag_samples"])
            for ch in a["c"]["per_channel"]
        )
        lines.append(
            f"| {item['stem']} | {r_lags} | "
            f"{a['rust']['selected_lag_samples'] if a['rust']['selected_lag_samples'] is not None else 'n/a'} | "
            f"{a['rust']['ambiguity_status']} | {c_lags} | "
            f"{a['c']['selected_lag_samples'] if a['c']['selected_lag_samples'] is not None else 'n/a'} | "
            f"{a['c']['ambiguity_status']} |"
        )
    lines.append("")
    lines.append(
        "Coarse stride / refine half-width (Rust, C): "
        + ", ".join(
            f"`{item['stem']}`={item['alignment']['rust']['coarse_step_samples']}/"
            f"{item['alignment']['rust']['per_channel'][0]['refine_half_width_samples']},"
            f"{item['alignment']['c']['coarse_step_samples']}/"
            f"{item['alignment']['c']['per_channel'][0]['refine_half_width_samples']}"
            for item in fixtures
        )
        + "."
    )
    lines.append("")

    decode_failures: list[str] = []
    channel_mismatches: list[str] = []
    rate_mismatches: list[str] = []
    length_implausible: list[str] = []
    length_unavailable: list[str] = []
    frame_equal = 0
    length_equal = 0
    for item in fixtures:
        for impl in ("rust", "c"):
            d = item["decoded"][impl]
            if d["decode_exit_code"] != 0 or not d["decode_stderr_empty"]:
                decode_failures.append(f"{item['stem']}/{impl}")
            if d["channels"] != item["input"]["channels"]:
                channel_mismatches.append(f"{item['stem']}/{impl}")
            if d["sample_rate_hz"] != item["input"]["sample_rate_hz"]:
                rate_mismatches.append(f"{item['stem']}/{impl}")
            length_status = d["length_check"]["status"]
            if length_status == "implausible":
                length_implausible.append(f"{item['stem']}/{impl}")
            elif length_status == "unavailable":
                length_unavailable.append(f"{item['stem']}/{impl}")
        if item["encoded"]["rust"]["frame_count"] == item["encoded"]["c"]["frame_count"]:
            frame_equal += 1
        if item["decoded"]["rust"]["samples_per_channel"] == item["decoded"]["c"]["samples_per_channel"]:
            length_equal += 1

    lines.append("## Structural validity")
    lines.append("")
    lines.append(
        "This section is separate from decoded-audio error and from encoded "
        "size. It reports only decode/structure gates."
    )
    lines.append("")
    lines.append("| Check | Result |")
    lines.append("|---|---|")
    lines.append(
        f"| Decodes with exit 0 and empty stderr | "
        f"{2 * len(fixtures) - len(decode_failures)}/{2 * len(fixtures)} |"
    )
    lines.append(
        f"| Decoded channel counts match manifest | "
        f"{2 * len(fixtures) - len(channel_mismatches)}/{2 * len(fixtures)} |"
    )
    lines.append(
        f"| Decoded sample rates match manifest | "
        f"{2 * len(fixtures) - len(rate_mismatches)}/{2 * len(fixtures)} |"
    )
    lines.append(
        f"| Decoded lengths equal frame_count×1024 − outputDelay | "
        f"{2 * len(fixtures) - len(length_implausible) - len(length_unavailable)}"
        f"/{2 * len(fixtures)} |"
    )
    if length_unavailable:
        lines.append(
            f"| Decoded-length check unavailable | {len(length_unavailable)} "
            f"({', '.join(length_unavailable)}) |"
        )
    lines.append(f"| Rust/C frame counts equal | {frame_equal}/{len(fixtures)} |")
    lines.append(
        f"| Rust/C decoded lengths equal | {length_equal}/{len(fixtures)} |"
    )
    clipped_r = sum(item["decoded"]["rust"]["clipped_samples"] for item in fixtures)
    clipped_c = sum(item["decoded"]["c"]["clipped_samples"] for item in fixtures)
    clipped_detail = ", ".join(
        f"{item['stem']}={item['decoded']['rust']['clipped_samples']}/"
        f"{item['decoded']['c']['clipped_samples']}"
        for item in fixtures
        if item["decoded"]["rust"]["clipped_samples"]
        or item["decoded"]["c"]["clipped_samples"]
    )
    lines.append(
        f"| Clipped samples (R/C total) | {clipped_r}/{clipped_c}"
        + (f" ({clipped_detail})" if clipped_detail else " (none)")
        + " |"
    )
    if decode_failures or channel_mismatches or rate_mismatches:
        lines.append("")
        lines.append(
            "Structural failures: "
            + ", ".join(decode_failures + channel_mismatches + rate_mismatches)
        )
    if length_implausible:
        lines.append("")
        lines.append(
            "Implausible decoded lengths (quality metrics excluded): "
            + ", ".join(length_implausible)
        )
    lines.append("")
    lines.append(
        "No encoder-internal diagnostics (PE, NMR, TNS decisions, stage dumps) "
        "are computed or used by this tool. They are not decoded-audio quality."
    )
    lines.append("")

    lines.append("## Encoded size and framing")
    lines.append("")
    lines.append(
        "These are transport/size measurements, not quality metrics. "
        "Bitrate uses the original fixture duration, not encoded duration."
    )
    lines.append("")
    lines.append(
        "| Fixture | Scope | Frames R/C | Bytes R/C | kbps R/C | "
        "Payload B R/C | Header B R/C |"
    )
    lines.append("|---|---|---:|---:|---:|---:|---:|")
    for item in fixtures:
        e = item["encoded"]
        lines.append(
            f"| {item['stem']} | {item['eligibility']['scope']} | "
            f"{e['rust']['frame_count']}/{e['c']['frame_count']} | "
            f"{e['rust']['total_bytes']}/{e['c']['total_bytes']} | "
            f"{e['rust']['total_bytes'] * 8 / item['input']['duration_seconds'] / 1000:.2f}/"
            f"{e['c']['total_bytes'] * 8 / item['input']['duration_seconds'] / 1000:.2f} | "
            f"{e['rust']['payload_bytes']}/{e['c']['payload_bytes']} | "
            f"{e['rust']['header_bytes']}/{e['c']['header_bytes']} |"
        )
    lines.append("")

    lines.append("## Decoded full-band error")
    lines.append("")
    lines.append(
        "Only full-band decoded error is computed. `sdr_db` is an alias of "
        "`snr_db` (`sdr_db_is_alias=true`), not an independent metric. "
        "No per-frame or per-band localization is included in F04-B1."
    )
    lines.append("")
    lines.append(
        "| Fixture | Scope | SNR R/C (dB) | Δ R−C (dB) | Residue (dB) | "
        "RMS err R/C (LSB) | Peak err R/C (LSB) | Overlap/ch R/C | "
        "Tail shortfall orig/test R/C |"
    )
    lines.append("|---|---|---:|---:|---:|---:|---:|---:|---|")
    for item in fixtures:
        q = item.get("quality")
        if q is None:
            continue
        r = q["rust_vs_original"]["aggregate"]
        c = q["c_vs_original"]["aggregate"]
        rc = q["rust_vs_c"]["aggregate"]
        r_snr = "n/a" if r["snr_db"] is None else f"{r['snr_db']:.2f}"
        c_snr = "n/a" if c["snr_db"] is None else f"{c['snr_db']:.2f}"
        delta = (
            "n/a"
            if r["snr_db"] is None or c["snr_db"] is None
            else f"{r['snr_db'] - c['snr_db']:+.2f}"
        )
        if rc["snr_db"] is not None:
            residue = f"{rc['snr_db']:.2f}"
        elif rc["snr_status"] == "identical":
            residue = "identical"
        else:
            residue = "n/a"
        lines.append(
            f"| {item['stem']} | {item['eligibility']['scope']} | "
            f"{r_snr}/{c_snr} | {delta} | {residue} | "
            f"{r['rms_error_lsb']:.2f}/{c['rms_error_lsb']:.2f} | "
            f"{r['peak_error_lsb']}/{c['peak_error_lsb']} | "
            f"{q['rust_vs_original']['overlap_samples_per_channel']}/"
            f"{q['c_vs_original']['overlap_samples_per_channel']} | "
            f"{q['rust_vs_original']['reference_tail_shortfall_samples_per_channel']}/"
            f"{q['rust_vs_original']['test_tail_shortfall_samples_per_channel']}; "
            f"{q['c_vs_original']['reference_tail_shortfall_samples_per_channel']}/"
            f"{q['c_vs_original']['test_tail_shortfall_samples_per_channel']} |"
        )
    excluded = [f for f in fixtures if f.get("quality") is None]
    if excluded:
        lines.append("")
        lines.append("Quality metrics excluded:")
        for item in excluded:
            lines.append(
                f"- `{item['stem']}`: {item['alignment']['rust']['ambiguity_status']} "
                f"(Rust), {item['alignment']['c']['ambiguity_status']} (C)"
            )
            for reason in item["eligibility"]["reasons"]:
                lines.append(f"  - {reason}")
    lines.append("")

    lines.append("## Eligible aggregate (descriptive only)")
    lines.append("")
    for scope in ("gated", "exploratory"):
        s = result["summary"][scope]
        lines.append(
            f"### {scope} ({s['count']} "
            f"fixture{'s' if s['count'] != 1 else ''})"
        )
        lines.append("")
        lines.append(
            f"- SNR agreement |R−C| (dB): {fmt_stats(s['snr_agreement_abs_db'])}"
        )
        lines.append(
            f"- Bitrate delta (%): {fmt_stats(s['bitrate_delta_percent'])}"
        )
        lines.append(
            f"- Residue SNR (dB): {fmt_stats(s['residue_snr_db'])}; "
            f"identical residues: {s.get('residue_identical_count', 0)}"
        )
        lines.append(
            f"- Equal frame counts: {s['frame_count_equal_count']}/{s['count']}; "
            f"equal decoded lengths: {s['decoded_length_equal_count']}/{s['count']}"
        )
        lines.append(
            f"- Stream provenance verified: "
            f"{s.get('stream_provenance_verified_count', 0)}/{s['count']}; "
            f"decoded length plausible: "
            f"{s.get('decoded_length_verified_count', 0)}/{s['count']}"
        )
        lines.append("")
        lines.append("All entries above are descriptive; no threshold is applied.")
        lines.append("")

    v3 = meta.get("v3_comparison")
    lines.append("## Retained V3 comparison")
    lines.append("")
    if v3 is None:
        lines.append("Not requested (`--v3-reference-dir` not supplied).")
    else:
        reproduction = meta.get("baseline_reproduction", {})
        reproduced = meta.get("baseline_reproduced")
        scope = reproduction.get("scope")
        if reproduced is None:
            reproduced_label = (
                "`not applicable (selection only)`"
                if scope == "selection"
                else "`not checked`"
            )
        else:
            reproduced_label = f"`{reproduced}`"
        lines.append(
            "Baseline reproduced (full 19-fixture corpus): "
            + reproduced_label
            + ". Selected artifacts matched: "
            + f"`{meta.get('baseline_reproduced_for_selection')}`"
            + f" (scope `{scope}`, {reproduction.get('selected_fixture_count')} "
            + f"fixture{'s' if reproduction.get('selected_fixture_count') != 1 else ''}). "
            + "This compares input, ADTS and decoded-WAV hashes "
            + "against the retained V3 artifacts. Missing historical files are "
            + "reported as unavailable and are not recreated. Behavioral "
            + "artifact reproduction is not a source-identical rebuild."
        )
        lines.append("")
        lines.append(f"{reproduction.get('note', '')}")
        lines.append("")
        lines.append("| Artifact | Matched / Compared / Expected |")
        lines.append("|---|---:|")
        for key, value in v3["summary"].items():
            lines.append(
                f"| {key} | {value['matched']} / {value['compared']} / "
                f"{value.get('expected', 'n/a')} |"
            )
        lines.append("")

    lines.append("## What this does not establish")
    lines.append("")
    lines.append(
        "- It does not establish perceptual equivalence, transparency, or "
        "audibility of any divergence."
    )
    lines.append(
        "- It does not turn SNR/SDR, bitrate, frame parity, or payload "
        "differences into a perceptual verdict."
    )
    lines.append(
        "- It does not include per-frame or per-band localization; that is "
        "reserved for F04-B2."
    )
    lines.append(
        "- 44.1 kHz results are exploratory until the benchmark-plan separate "
        "alignment gate is satisfied."
    )
    lines.append(
        "- The m8 fixture remains PROVISIONAL and is not in the gated aggregate."
    )
    lines.append(
        "- `gated` requires verified per-stream provenance; it is not implied by "
        "a matching encoder binary that was never run or by a supplied ADTS "
        "directory."
    )
    lines.append(
        "- A retained-V3 match trusts the supplied `--v3-reference-dir` as the "
        "baseline; the tool checks byte equality, not the historical "
        "authenticity of that directory."
    )
    lines.append(
        "- `baseline_reproduced=true` (full corpus) is behavioral artifact "
        "reproduction, not a source-identical Rust rebuild and not proof of "
        "perceptual quality."
    )
    lines.append(
        "- The decoded-length check is exact for the retained FDK CLI; it does "
        "not establish tail/flush correctness beyond that identity."
    )
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="AAC-LC VBR-5 C-vs-Rust objective metrics (measurement only)"
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        default=Path(__file__).resolve().parent / "fixtures-v3.json",
        help="fixture manifest JSON (default: fixtures-v3.json next to this script)",
    )
    parser.add_argument(
        "--work-dir",
        type=Path,
        help="scratch/output directory; generated inputs, ADTS, WAVs, report and JSON",
    )
    parser.add_argument("--c-enc-bin", type=Path, help="exact C aac-enc binary")
    parser.add_argument("--c-dec-bin", type=Path, help="exact C aac-dec binary")
    parser.add_argument("--c-ref-dir", type=Path, help="C reference git checkout")
    parser.add_argument("--c-lib", type=Path, help="FDK AAC shared library used by the C binaries")
    parser.add_argument("--rust-adts-dir", type=Path, help="use pre-generated Rust ADTS")
    parser.add_argument("--c-adts-dir", type=Path, help="use pre-generated C ADTS")
    parser.add_argument(
        "--v3-reference-dir",
        type=Path,
        help="retained V3 artifact root for optional hash comparison (not recreated if missing)",
    )
    parser.add_argument(
        "--separate-probe-input",
        type=Path,
        help="optional separate mono/44.1 kHz probe WAV for declared-hash validation",
    )
    parser.add_argument(
        "--only",
        help="comma-separated corpus stems for a subset run",
    )
    parser.add_argument(
        "--allow-different-reference",
        action="store_true",
        help="allow non-recorded C binaries/source; all results become exploratory",
    )
    parser.add_argument(
        "--validate-manifest",
        action="store_true",
        help="validate manifest shape and input hashes, then exit",
    )
    parser.add_argument("--report", type=Path, help="report path (default: work-dir/report.md)")
    parser.add_argument("--json", type=Path, help="JSON path (default: work-dir/metrics.json)")
    args = parser.parse_args(argv)

    try:
        manifest_path = args.manifest.resolve()
        manifest = load_json(manifest_path)
        validate_manifest(manifest)
        manifest_sha = sha256_file(manifest_path)
        probe_validation = verify_separate_probes(
            manifest,
            explicit_path=(
                args.separate_probe_input.resolve()
                if args.separate_probe_input
                else None
            ),
        )

        only = None
        if args.only:
            only = [part.strip() for part in args.only.split(",") if part.strip()]
            if not only:
                raise MetricsError("--only was provided but contained no stems")
        fixtures = selected_fixtures(manifest, only)

        if args.validate_manifest:
            work_dir = args.work_dir.resolve() if args.work_dir else Path(
                tempfile.mkdtemp(prefix="aac-vbr5-manifest-")
            )
            work_dir.mkdir(parents=True, exist_ok=True)
            inputs = prepare_inputs(fixtures, repo_root=Path(__file__).resolve().parents[2], work_dir=work_dir)
            print(
                f"manifest OK: {len(fixtures)} selected corpus fixtures; "
                f"{len(inputs)} input hashes matched"
            )
            for probe in probe_validation:
                if probe["status"] == "matched":
                    print(
                        f"separate probe {probe['stem']}: declared hash verified"
                    )
                else:
                    print(
                        f"separate probe {probe['stem']}: hash unavailable "
                        f"({probe['note']})"
                    )
            return 0

        if args.work_dir is None:
            raise MetricsError("--work-dir is required for a measurement run")
        if args.c_dec_bin is None:
            raise MetricsError(
                "no C decoder was supplied. The baseline needs the exact f14ad40 "
                "aac-dec binary; pass --c-dec-bin, or use the retained V3 ref-build."
            )
        if args.c_enc_bin is None and args.c_adts_dir is None:
            raise MetricsError(
                "no C encoder or C ADTS directory was supplied. Pass --c-enc-bin "
                "or --c-adts-dir; the tool will not substitute a different reference."
            )

        work_dir = args.work_dir.resolve()
        work_dir.mkdir(parents=True, exist_ok=True)
        repo_root = Path(__file__).resolve().parents[2]

        inputs = prepare_inputs(fixtures, repo_root=repo_root, work_dir=work_dir)
        rust_meta = rust_metadata(repo_root, manifest["baseline"])
        c_meta = c_reference_metadata(
            manifest,
            c_enc_bin=args.c_enc_bin.resolve() if args.c_enc_bin else None,
            c_dec_bin=args.c_dec_bin.resolve(),
            c_ref_dir=args.c_ref_dir.resolve() if args.c_ref_dir else None,
            c_lib=args.c_lib.resolve() if args.c_lib else None,
            allow_different_reference=args.allow_different_reference,
        )

        rust_adts, rust_collection = collect_rust_adts(
            fixtures,
            repo_root=repo_root,
            work_dir=work_dir,
            rust_adts_dir=args.rust_adts_dir.resolve() if args.rust_adts_dir else None,
        )
        c_adts, c_collection = collect_c_adts(
            fixtures,
            inputs,
            manifest=manifest,
            work_dir=work_dir,
            c_enc_bin=args.c_enc_bin.resolve() if args.c_enc_bin else None,
            c_adts_dir=args.c_adts_dir.resolve() if args.c_adts_dir else None,
            allow_different_reference=args.allow_different_reference,
        )

        decoded_dir = work_dir / "decoded"
        decoded_dir.mkdir(parents=True, exist_ok=True)
        decoded: dict[str, dict[str, Path]] = {}
        decoded_meta: dict[str, dict[str, dict[str, Any]]] = {}
        for fixture in fixtures:
            stem = fixture["stem"]
            decoded[stem] = {}
            decoded_meta[stem] = {}
            for impl, adts_path in (("rust", rust_adts[stem]), ("c", c_adts[stem])):
                wav_path = decoded_dir / f"{stem}.{impl}.wav"
                decoded_meta[stem][impl] = decode_stream(
                    args.c_dec_bin.resolve(),
                    adts_path,
                    wav_path,
                    expected_channels=fixture["channels"],
                    expected_sample_rate=fixture["sample_rate_hz"],
                )
                decoded[stem][impl] = wav_path

        v3_comparison = compare_v3(
            fixtures,
            v3_dir=args.v3_reference_dir.resolve() if args.v3_reference_dir else None,
            inputs=inputs,
            rust_adts=rust_adts,
            c_adts=c_adts,
            decoded=decoded,
        )

        results = []
        for fixture in fixtures:
            stem = fixture["stem"]
            v3_item = (
                v3_comparison["per_fixture"][stem]
                if v3_comparison is not None
                else None
            )
            provenance = artifact_provenance(
                stem,
                rust_meta=rust_meta,
                rust_collection=rust_collection,
                c_meta=c_meta,
                c_collection=c_collection,
                v3_item=v3_item,
                allow_different_reference=args.allow_different_reference,
            )
            results.append(
                build_result(
                    fixture,
                    input_path=inputs[stem],
                    rust_adts_path=rust_adts[stem],
                    c_adts_path=c_adts[stem],
                    decoded_meta=decoded_meta[stem],
                    alignment_contract=manifest["alignment"],
                    provenance=provenance,
                    decoder_output_delays=manifest["decoder_output_delay_samples"],
                    frame_length_samples=manifest["encoder_configuration"][
                        "frame_length_samples"
                    ],
                    v3_item=v3_item,
                )
            )

        probe_by_stem = {item["stem"]: item for item in probe_validation}
        separate = [
            {
                "stem": f["stem"],
                "category": f["category"],
                "provenance_status": f["provenance_status"],
                "sample_rate_hz": f["sample_rate_hz"],
                "channels": f["channels"],
                "input_sha256": f["input_sha256"],
                "input_hash_status": probe_by_stem.get(f["stem"], {}).get(
                    "status", "unavailable"
                ),
                "input_hash_source": probe_by_stem.get(f["stem"], {}).get(
                    "source", "unavailable"
                ),
                "input_sha256_actual": probe_by_stem.get(f["stem"], {}).get(
                    "actual_sha256"
                ),
                "measured": False,
                "reason": f["eligibility"]["reason"],
            }
            for f in manifest["fixtures"]
            if not f["corpus_member"]
        ]

        summary = {
            "measured_streams": len(results),
            "gated": summary_for_scope(results, "gated"),
            "exploratory": summary_for_scope(results, "exploratory"),
            "excluded": {
                "count": len([r for r in results if r["eligibility"]["scope"] == "excluded"]),
                "stems": [
                    r["stem"] for r in results if r["eligibility"]["scope"] == "excluded"
                ],
            },
            "separate_probes": separate,
            "subset": len(fixtures) != 19,
            "selection": only,
            "stream_provenance": {
                "rust_verified": len(
                    [r for r in results if r["eligibility"]["rust_provenance"]["verified"]]
                ),
                "c_verified": len(
                    [r for r in results if r["eligibility"]["c_provenance"]["verified"]]
                ),
                "both_verified": len(
                    [
                        r
                        for r in results
                        if r["eligibility"]["stream_provenance_verified"]
                    ]
                ),
            },
        }
        scope_total = (
            summary["gated"]["count"]
            + summary["exploratory"]["count"]
            + summary["excluded"]["count"]
        )
        scope_stems = (
            summary["gated"]["stems"]
            + summary["exploratory"]["stems"]
            + summary["excluded"]["stems"]
        )
        if scope_total != len(results) or sorted(scope_stems) != sorted(
            r["stem"] for r in results
        ):
            raise MetricsError(
                "internal error: eligibility scope counts do not reconcile with "
                "per-fixture results"
            )

        baseline_status = baseline_reproduction_status(fixtures, v3_comparison)
        baseline_reproduced = baseline_status["baseline_reproduced"]

        result = {
            "schema_version": 1,
            "tool": {"name": TOOL_NAME, "version": TOOL_VERSION},
            "manifest": {
                "name": manifest["name"],
                "corpus_version": manifest["corpus_version"],
                "sha256": manifest_sha,
            },
            "run": {
                "rust": {
                    **rust_meta,
                    "adts_source": rust_collection["adts_source"],
                    "harness_ran": rust_collection["harness_ran"],
                },
                "c": {
                    **c_meta,
                    "adts_source": c_collection["adts_source"],
                    "encoder_ran": c_collection["encoder_ran"],
                },
                "python_version": sys.version.split()[0],
                "decoder_output_delay_samples": manifest["decoder_output_delay_samples"],
                "alignment_contract": {
                    "nominal_lag_samples": manifest["alignment"]["nominal_lag_samples"],
                    "search_half_width_samples": manifest["alignment"]["search_half_width_samples"],
                    "channel_tolerance_samples": manifest["alignment"]["channel_tolerance_samples"],
                    "refine_half_width_samples": manifest["alignment"]["refine_half_width_samples"],
                    "long_signal_refine_half_width_samples": manifest["alignment"]["long_signal_refine_half_width_samples"],
                    "sign_convention": manifest["alignment"]["sign_convention"],
                    "delay_accounting": manifest["alignment"]["delay_accounting"],
                    "frame_mapping": manifest["alignment"]["frame_mapping"],
                },
                "baseline_reproduced": baseline_reproduced,
                "baseline_reproduced_for_selection": baseline_status[
                    "baseline_reproduced_for_selection"
                ],
                "baseline_reproduction": baseline_status,
                "v3_comparison": v3_comparison,
                "reference_exact": c_meta["reference_exact"],
            },
            "interpretation": {
                "perceptual_claim": False,
                "thresholds_used": False,
                "sdr_db_is_alias": True,
                "quality_metric": "full-band decoded error vs original PCM",
                "per_frame_or_per_band": False,
                "abx_or_listening_test": False,
            },
            "fixtures": results,
            "separate_probes": separate,
            "summary": summary,
        }

        report_path = args.report.resolve() if args.report else work_dir / "report.md"
        json_path = args.json.resolve() if args.json else work_dir / "metrics.json"
        report_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(
            json.dumps(result, indent=2, sort_keys=False) + "\n", encoding="utf-8"
        )
        report_path.write_text(render_report(result), encoding="utf-8")
        print(f"wrote report: {report_path}")
        print(f"wrote JSON:   {json_path}")
        print(
            f"gated={summary['gated']['count']} "
            f"exploratory={summary['exploratory']['count']} "
            f"excluded={summary['excluded']['count']} "
            f"separate={len(separate)}"
        )
        print(
            "stream provenance verified "
            f"(Rust={summary['stream_provenance']['rust_verified']}, "
            f"C={summary['stream_provenance']['c_verified']}, "
            f"both={summary['stream_provenance']['both_verified']}) "
            f"of {len(results)}"
        )
        downgraded = [
            r
            for r in results
            if r["eligibility"]["static_gated_aggregate"]
            and r["eligibility"]["scope"] == "exploratory"
        ]
        if downgraded:
            print(
                f"warning: {len(downgraded)} statically gated fixture(s) are "
                "exploratory in this run; see per-fixture provenance and "
                "eligibility reasons in the report/JSON",
                file=sys.stderr,
            )
        if baseline_reproduced is False:
            print(
                "warning: full 19-fixture baseline reproduction was not "
                "established; at least one artifact class is incomplete or "
                "mismatched (no historical hash was recreated)",
                file=sys.stderr,
            )
        elif baseline_reproduced is None and v3_comparison is not None:
            print(
                "note: baseline_reproduced is not applicable to a subset; see "
                "baseline_reproduced_for_selection",
                file=sys.stderr,
            )
        return 0
    except MetricsError as exc:
        print(f"{TOOL_NAME}: error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
