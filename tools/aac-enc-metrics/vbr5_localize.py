#!/usr/bin/env python3
"""AAC-LC VBR-5 decoded-audio error localization (F04-B2, diagnostic only).

This tool localizes Rust-vs-C decoded-audio differences in time (1024-sample
analysis windows plus coarser fixed regions) and, optionally, in frequency
(stdlib-only Hann-windowed DFT band energies). It is a diagnostic measurement
aid, not a quality ranking: it applies no pass/fail thresholds, claims no
perceptual equivalence or audibility, and computes no single quality score.

It does NOT change encoder/decoder code and does NOT change the B1 metric
definitions, gating policy, manifest eligibility, or alignment policy. All
fixture selection, provenance checks, alignment estimation, decoded-length
checks, and full-band error formulas are reused verbatim from
``vbr5_metrics.py``. A fixture B1 excludes (``quality is None``) is never
analyzed here; its B1 scope and reason are passed through unchanged.

Frame-to-time mapping (analysis windowing, NOT a codec mapping):
  * Window ``k`` covers original-PCM samples ``[k*W, min((k+1)*W, overlap))``
    where ``W`` is ``--window-samples`` (default 1024) and ``overlap`` is the
    joint B1-style overlap ``min(rust_dec_len - lag_r, c_dec_len - lag_c,
    orig_len)``. Both implementations are compared against the SAME original
    samples; the Rust-vs-C difference compares the decoded samples aligned to
    that same original interval (``rust[lag_r+o]`` vs ``c[lag_c+o]``).
  * The first window is full (starts at original sample 0) unless the overlap
    itself is shorter than one window. The last window is partial whenever
    ``overlap % W != 0`` and is flagged ``partial: true``; its metrics use
    only its valid samples.
  * A decoded analysis window does NOT map to one AAC encoder frame. Encoder
    delay, MDCT overlap-add, and decoder priming mean decoded sample ``n``
    cannot be attributed to a single source frame. Time intervals (seconds)
    are ``sample / sample_rate`` in original-sample coordinates.

Spectral mode (``--enable-spectral``, stdlib only, diagnostic):
  * Transform: N-point DFT (radix-2 FFT, N == window size, power of two
    required) of the Hann-windowed error signal, per channel.
  * Window: Hann ``w[n] = 0.5*(1-cos(2*pi*n/(N-1)))``.
  * Bands (Hz): [0,1000), [1000,4000), [4000,8000), [8000,12000),
    [12000,Nyquist]. Bin ``k`` covers ``k*rate/N`` Hz; band ``[lo,hi)`` uses
    bins ``ceil(lo*N/rate)`` through ``ceil(hi*N/rate)-1`` (Nyquist bin
    inclusive in the top band).
  * Energy: one-sided ``sum |X[k]|^2`` over the band's bins (DC..Nyquist),
    with interior bins (``0 < k < N/2``) doubled so band energies sum to the
    Parseval total ``N * sum((e[n]*w[n])^2)``. Identical parameters for
    rust-err, c-err, and rust-vs-c diff signals.
  * Spectral analysis covers full windows only; partial windows are skipped
    with ``spectral_status: partial_skipped``.
  * These are windowed error energies, NOT masking-weighted errors. No metric
    here is called NMR and no masking model is implemented.
"""

from __future__ import annotations

import argparse
import cmath
import json
import math
import sys
from pathlib import Path
from typing import Any

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vbr5_metrics as b1  # noqa: E402  (reuse, never duplicate, B1 policy)

TOOL_NAME = "ferrite-aac-vbr5-localize"
TOOL_VERSION = "1.0.0"
DEFAULT_WINDOW_SAMPLES = 1024
DEFAULT_REGION_SAMPLES = 4096

# Frequency bands in Hz; the last upper edge means Nyquist (rate/2).
BAND_EDGES_HZ = (0.0, 1000.0, 4000.0, 8000.0, 12000.0, None)


class LocalizeError(RuntimeError):
    """A clear, actionable localization failure."""


# --------------------------------------------------------------------------
# Small pure-stdlib DSP helpers (spectral mode only)
# --------------------------------------------------------------------------

def hann_window(n: int) -> list[float]:
    if n < 2:
        raise LocalizeError(f"Hann window requires at least 2 points, got {n}")
    return [0.5 * (1.0 - math.cos(2.0 * math.pi * i / (n - 1))) for i in range(n)]


def is_power_of_two(n: int) -> bool:
    return n > 0 and (n & (n - 1)) == 0


def fft_forward(values: list[complex]) -> list[complex]:
    """Radix-2 decimation-in-time FFT (forward, unscaled)."""
    n = len(values)
    if not is_power_of_two(n):
        raise LocalizeError(f"FFT length must be a power of two, got {n}")
    spectrum = list(values)
    # Bit-reversal permutation.
    bits = n.bit_length() - 1
    for i in range(n):
        rev = 0
        for b in range(bits):
            rev |= ((i >> b) & 1) << (bits - 1 - b)
        if rev > i:
            spectrum[i], spectrum[rev] = spectrum[rev], spectrum[i]
    length = 2
    while length <= n:
        angle = -2.0 * math.pi / length
        root = cmath.exp(complex(0.0, angle))
        for start in range(0, n, length):
            twiddle = complex(1.0, 0.0)
            for j in range(length // 2):
                u = spectrum[start + j]
                v = spectrum[start + j + length // 2] * twiddle
                spectrum[start + j] = u + v
                spectrum[start + j + length // 2] = u - v
                twiddle *= root
        length *= 2
    return spectrum


def band_bin_ranges(n: int, sample_rate_hz: int) -> list[dict[str, Any]]:
    """Map BAND_EDGES_HZ to inclusive DFT bin ranges for an N-point DFT."""
    nyquist = sample_rate_hz / 2.0
    edges: list[float] = [e if e is not None else nyquist for e in BAND_EDGES_HZ]
    ranges: list[dict[str, Any]] = []
    for lo, hi in zip(edges[:-1], edges[1:]):
        k_lo = int(math.ceil(lo * n / sample_rate_hz))
        if hi >= nyquist:
            k_hi = n // 2  # Nyquist bin inclusive
        else:
            k_hi = int(math.ceil(hi * n / sample_rate_hz)) - 1
        k_lo = max(k_lo, 0)
        k_hi = min(k_hi, n // 2)
        ranges.append({
            "band_hz": [lo, hi if hi < nyquist else nyquist],
            "bin_lo": k_lo,
            "bin_hi": k_hi,
            "bin_count": max(0, k_hi - k_lo + 1),
        })
    return ranges


def spectral_band_energies(
    error: list[int],
    window: list[float],
    sample_rate_hz: int,
) -> dict[str, Any]:
    """Hann-windowed one-sided DFT band energies of an error signal."""
    n = len(error)
    if n != len(window):
        raise LocalizeError("error signal and window lengths differ")
    if not is_power_of_two(n):
        raise LocalizeError(f"spectral analysis needs a power-of-two length, got {n}")
    weighted = [float(e) * w for e, w in zip(error, window)]
    time_energy = sum(v * v for v in weighted)
    spectrum = fft_forward([complex(v, 0.0) for v in weighted])
    half = n // 2
    # One-sided bin energies: interior bins doubled so the bands sum to the
    # Parseval total N * time_energy for real-valued inputs.
    magnitudes_sq = []
    for k in range(half + 1):
        energy = abs(spectrum[k]) ** 2
        if 0 < k < half:
            energy *= 2.0
        magnitudes_sq.append(energy)
    total_spectral = sum(magnitudes_sq)
    expected_spectral = n * time_energy
    if expected_spectral > 0.0:
        conservation_rel_err = abs(total_spectral - expected_spectral) / expected_spectral
    else:
        conservation_rel_err = 0.0 if total_spectral == 0.0 else float("inf")
    bands = band_bin_ranges(n, sample_rate_hz)
    band_energies: list[float] = []
    for band in bands:
        energy = sum(magnitudes_sq[band["bin_lo"]:band["bin_hi"] + 1])
        band_energies.append(energy)
    return {
        "n_points": n,
        "window": "hann",
        "time_energy_hann_weighted": time_energy,
        "total_spectral_energy": total_spectral,
        "parseval_expected_total": expected_spectral,
        "parseval_relative_error": conservation_rel_err,
        "bands": [
            {
                "band_hz": band["band_hz"],
                "bin_lo": band["bin_lo"],
                "bin_hi": band["bin_hi"],
                "energy": energy,
            }
            for band, energy in zip(bands, band_energies)
        ],
    }


# --------------------------------------------------------------------------
# Windowed error statistics (same formulas as B1 error_stats)
# --------------------------------------------------------------------------

def window_error_stats(
    reference: list[int],
    test: list[int],
) -> dict[str, Any]:
    """Per-window error stats with B1-identical SNR/RMS/peak formulas.

    Returns display values (rounded like B1) plus raw energies for exact
    aggregation. An empty slice yields status ``insufficient_overlap``.
    """
    n = len(reference)
    if n == 0 or len(test) == 0:
        return {
            "snr_db": None,
            "snr_status": "insufficient_overlap",
            "rms_error_lsb": None,
            "peak_error_lsb": None,
            "signal_rms_lsb": None,
            "signal_peak_lsb": None,
            "samples_compared": 0,
            "ref_energy": 0.0,
            "error_energy": 0.0,
        }
    if len(reference) != len(test):
        raise LocalizeError("window reference/test lengths differ")
    # Reuse B1's verified formula for the display values.
    display = b1.error_stats(list(reference), list(test))
    ref_energy = 0.0
    error_energy = 0.0
    for a, b in zip(reference, test):
        ref_energy += float(a) * a
        diff = a - b
        error_energy += float(diff) * diff
    return {
        "snr_db": display["snr_db"],
        "snr_status": display["snr_status"],
        "rms_error_lsb": display["rms_error_lsb"],
        "peak_error_lsb": display["peak_error_lsb"],
        "signal_rms_lsb": display["signal_rms_lsb"],
        "signal_peak_lsb": display["signal_peak_lsb"],
        "samples_compared": n,
        "ref_energy": ref_energy,
        "error_energy": error_energy,
    }


def aggregate_stats(
    ref_energy: float,
    error_energy: float,
    samples: int,
    peak_error: int,
    signal_peak: int,
) -> dict[str, Any]:
    """Combine raw energies with B1-identical SNR/RMS formulas."""
    if samples <= 0:
        return {
            "snr_db": None,
            "snr_status": "insufficient_overlap",
            "rms_error_lsb": None,
            "peak_error_lsb": None,
            "signal_rms_lsb": None,
            "signal_peak_lsb": None,
            "samples_compared": 0,
        }
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
        "snr_status": snr_status,
        "rms_error_lsb": round(math.sqrt(error_energy / samples), 6),
        "peak_error_lsb": peak_error,
        "signal_rms_lsb": round(math.sqrt(ref_energy / samples), 6),
        "signal_peak_lsb": signal_peak,
        "samples_compared": samples,
    }


# --------------------------------------------------------------------------
# Artifact loading with B1 provenance reuse
# --------------------------------------------------------------------------

def resolve_wave_paths(
    stem: str,
    *,
    b1_work_dir: Path | None,
    v3_reference_dir: Path | None,
) -> dict[str, Path]:
    if b1_work_dir is not None:
        return {
            "original": b1_work_dir / "inputs" / f"{stem}.wav",
            "rust": b1_work_dir / "decoded" / f"{stem}.rust.wav",
            "c": b1_work_dir / "decoded" / f"{stem}.c.wav",
            "source_label": "b1-work-dir",
        }
    assert v3_reference_dir is not None
    return {
        "original": v3_reference_dir / "inputs" / f"{stem}.wav",
        "rust": v3_reference_dir / "rust-wav" / f"{stem}.rust.wav",
        "c": v3_reference_dir / "c-wav" / f"{stem}.c.wav",
        "source_label": "retained-v3",
    }


def check_file(path: Path, role: str) -> Path:
    if not path.is_file():
        raise LocalizeError(f"missing {role} artifact: {path}")
    return path


def load_triplet(
    stem: str,
    manifest_fixture: dict[str, Any],
    b1_fixture: dict[str, Any],
    *,
    b1_work_dir: Path | None,
    v3_reference_dir: Path | None,
) -> dict[str, Any]:
    """Load and provenance-check one fixture's three WAVs.

    Verifies the original input hash against the manifest and the decoded
    hashes against the B1 JSON. When ``--v3-reference-dir`` is the wave
    source, the files ARE the retained artifacts; when it is additionally
    supplied alongside ``--b1-work-dir``, decoded hashes are cross-checked
    against the retained V3 files without trusting directory layout alone.
    """
    paths = resolve_wave_paths(
        stem, b1_work_dir=b1_work_dir, v3_reference_dir=v3_reference_dir
    )
    for role in ("original", "rust", "c"):
        check_file(paths[role], role)
    original = b1.read_wav_i16(paths["original"])
    rust = b1.read_wav_i16(paths["rust"])
    c = b1.read_wav_i16(paths["c"])

    for name, wav in (("original", original), ("rust", rust), ("c", c)):
        if wav["sample_rate_hz"] != manifest_fixture["sample_rate_hz"]:
            raise LocalizeError(
                f"{stem}/{name}: sample rate {wav['sample_rate_hz']} != "
                f"manifest {manifest_fixture['sample_rate_hz']}"
            )
        if wav["channels"] != manifest_fixture["channels"]:
            raise LocalizeError(
                f"{stem}/{name}: channels {wav['channels']} != "
                f"manifest {manifest_fixture['channels']}"
            )
    if original["samples_per_channel"] != manifest_fixture["samples_per_channel"]:
        raise LocalizeError(f"{stem}: original sample count != manifest")
    if rust["channels"] != c["channels"] or rust["channels"] != original["channels"]:
        raise LocalizeError(f"{stem}: channel mismatch between WAVs")

    actual_input_sha = b1.sha256_file(paths["original"])
    if actual_input_sha != manifest_fixture["input_sha256"]:
        raise LocalizeError(f"{stem}: input SHA-256 mismatch vs manifest")

    b1_rust_sha = b1_fixture["decoded"]["rust"]["wav_sha256"]
    b1_c_sha = b1_fixture["decoded"]["c"]["wav_sha256"]
    rust_sha = b1.sha256_file(paths["rust"])
    c_sha = b1.sha256_file(paths["c"])
    if rust_sha != b1_rust_sha:
        raise LocalizeError(
            f"{stem}: rust decoded WAV does not match the B1 JSON hash "
            f"(expected {b1_rust_sha[:16]}.., got {rust_sha[:16]}..)"
        )
    if c_sha != b1_c_sha:
        raise LocalizeError(
            f"{stem}: c decoded WAV does not match the B1 JSON hash "
            f"(expected {b1_c_sha[:16]}.., got {c_sha[:16]}..)"
        )

    v3_match: dict[str, Any] | None = None
    if v3_reference_dir is not None and b1_work_dir is not None:
        # Cross-check B1 workdir files against retained V3 bytes.
        v3_paths = resolve_wave_paths(
            stem, b1_work_dir=None, v3_reference_dir=v3_reference_dir
        )
        v3_match = {}
        for role in ("original", "rust", "c"):
            v3_file = v3_paths[role]
            if not v3_file.is_file():
                v3_match[role] = None
            else:
                v3_match[role] = b1.sha256_file(v3_file) == b1.sha256_file(paths[role])
    elif v3_reference_dir is not None:
        v3_match = {"original": True, "rust": True, "c": True}

    return {
        "original": original,
        "rust": rust,
        "c": c,
        "paths": {k: str(v) for k, v in paths.items()},
        "rust_sha256": rust_sha,
        "c_sha256": c_sha,
        "v3_match": v3_match,
    }


# --------------------------------------------------------------------------
# Per-fixture localization
# --------------------------------------------------------------------------

def localize_fixture(
    manifest_fixture: dict[str, Any],
    b1_fixture: dict[str, Any],
    triplet: dict[str, Any],
    alignment_contract: dict[str, Any],
    decoder_output_delays: dict[str, Any],
    frame_length_samples: int,
    *,
    window_samples: int,
    region_samples: int,
    enable_spectral: bool,
) -> dict[str, Any]:
    stem = manifest_fixture["stem"]
    rate = int(manifest_fixture["sample_rate_hz"])
    channels = int(manifest_fixture["channels"])
    original = triplet["original"]
    rust = triplet["rust"]
    c = triplet["c"]

    # Reuse B1 alignment verbatim, then require consistency with the B1 run.
    align_rust = b1.estimate_alignment(original, rust, alignment_contract)
    align_c = b1.estimate_alignment(original, c, alignment_contract)
    b1_lag_rust = b1_fixture["alignment"]["rust"]["selected_lag_samples"]
    b1_lag_c = b1_fixture["alignment"]["c"]["selected_lag_samples"]
    lag_rust = align_rust["selected_lag_samples"]
    lag_c = align_c["selected_lag_samples"]
    if lag_rust != b1_lag_rust or lag_c != b1_lag_c:
        raise LocalizeError(
            f"{stem}: recomputed alignment lags ({lag_rust}/{lag_c}) differ from "
            f"the B1 JSON ({b1_lag_rust}/{b1_lag_c}); refusing to analyze under "
            "a different alignment policy"
        )
    if lag_rust is None or lag_c is None:
        raise LocalizeError(f"{stem}: B1-excluded fixture reached analysis")

    # Reuse B1 decoded-length checks verbatim.
    length_checks: dict[str, dict[str, Any]] = {}
    for impl, wav, adts_key in (
        ("rust", rust, "rust"),
        ("c", c, "c"),
    ):
        frame_count = int(b1_fixture["encoded"][adts_key]["frame_count"])
        length_checks[impl] = b1.decoded_length_check(
            frame_count=frame_count,
            decoded_samples_per_channel=wav["samples_per_channel"],
            sample_rate_hz=rate,
            output_delays=decoder_output_delays,
            frame_length_samples=frame_length_samples,
        )

    # Joint overlap in original-sample coordinates (B1-style per impl, then
    # the minimum so both implementations share identical original samples).
    rust_overlap = min(
        rust["samples_per_channel"] - lag_rust,
        original["samples_per_channel"],
    )
    c_overlap = min(
        c["samples_per_channel"] - lag_c,
        original["samples_per_channel"],
    )
    joint_overlap = min(rust_overlap, c_overlap)

    window_bounds: list[tuple[int, int]] = []
    start = 0
    while start < joint_overlap:
        window_bounds.append((start, min(start + window_samples, joint_overlap)))
        start += window_samples

    region_bounds: list[tuple[int, int]] = []
    start = 0
    while start < joint_overlap:
        region_bounds.append((start, min(start + region_samples, joint_overlap)))
        start += region_samples

    hann = hann_window(window_samples) if enable_spectral else None

    per_channel: list[dict[str, Any]] = []
    for channel in range(channels):
        orig_ch = original["pcm"][channel]
        rust_ch = rust["pcm"][channel]
        c_ch = c["pcm"][channel]

        windows: list[dict[str, Any]] = []
        acc = {
            "rust": {"ref": 0.0, "err": 0.0, "peak": 0, "sig_peak": 0, "n": 0},
            "c": {"ref": 0.0, "err": 0.0, "peak": 0, "sig_peak": 0, "n": 0},
            "diff": {"ref": 0.0, "err": 0.0, "peak": 0, "sig_peak": 0, "n": 0},
        }
        spectral_totals = (
            {"rust_err": 0.0, "c_err": 0.0, "diff": 0.0, "full_windows": 0}
            if enable_spectral
            else None
        )
        for index, (w0, w1) in enumerate(window_bounds):
            valid = w1 - w0
            partial = valid < window_samples
            orig_slice = orig_ch[w0:w1]
            rust_slice = rust_ch[lag_rust + w0:lag_rust + w1]
            c_slice = c_ch[lag_c + w0:lag_c + w1]
            if len(rust_slice) != valid or len(c_slice) != valid:
                raise LocalizeError(f"{stem}/ch{channel}/w{index}: slice shortfall")
            rust_stats = window_error_stats(orig_slice, rust_slice)
            c_stats = window_error_stats(orig_slice, c_slice)
            diff_stats = window_error_stats(rust_slice, c_slice)
            for key, stats in (
                ("rust", rust_stats),
                ("c", c_stats),
                ("diff", diff_stats),
            ):
                acc[key]["ref"] += stats["ref_energy"]
                acc[key]["err"] += stats["error_energy"]
                acc[key]["n"] += stats["samples_compared"]
                if stats["peak_error_lsb"] is not None:
                    acc[key]["peak"] = max(acc[key]["peak"], stats["peak_error_lsb"])
                # diff reference peak is the rust-decoded peak; reuse B1's
                # signal_peak_lsb of the reference slice.
                if stats["signal_peak_lsb"] is not None:
                    acc[key]["sig_peak"] = max(
                        acc[key]["sig_peak"], stats["signal_peak_lsb"]
                    )
            entry: dict[str, Any] = {
                "window_index": index,
                "orig_start_sample": w0,
                "orig_end_sample": w1,
                "valid_samples": valid,
                "start_time_s": round(w0 / rate, 6),
                "end_time_s": round(w1 / rate, 6),
                "partial": partial,
                "rust_vs_original": {
                    k: rust_stats[k]
                    for k in (
                        "snr_db",
                        "snr_status",
                        "rms_error_lsb",
                        "peak_error_lsb",
                        "signal_rms_lsb",
                        "signal_peak_lsb",
                        "samples_compared",
                    )
                },
                "c_vs_original": {
                    k: c_stats[k]
                    for k in (
                        "snr_db",
                        "snr_status",
                        "rms_error_lsb",
                        "peak_error_lsb",
                        "signal_rms_lsb",
                        "signal_peak_lsb",
                        "samples_compared",
                    )
                },
                "rust_vs_c": {
                    "snr_db": diff_stats["snr_db"],
                    "snr_status": diff_stats["snr_status"],
                    "rms_difference_lsb": diff_stats["rms_error_lsb"],
                    "peak_difference_lsb": diff_stats["peak_error_lsb"],
                    "samples_compared": diff_stats["samples_compared"],
                },
            }
            if enable_spectral:
                assert hann is not None
                if partial:
                    entry["spectral"] = {"spectral_status": "partial_skipped"}
                else:
                    rust_err = [a - b for a, b in zip(rust_slice, orig_slice)]
                    c_err = [a - b for a, b in zip(c_slice, orig_slice)]
                    diff_err = [a - b for a, b in zip(rust_slice, c_slice)]
                    spec = {
                        "spectral_status": "computed",
                        "rust_err": spectral_band_energies(rust_err, hann, rate),
                        "c_err": spectral_band_energies(c_err, hann, rate),
                        "diff": spectral_band_energies(diff_err, hann, rate),
                    }
                    entry["spectral"] = spec
                    assert spectral_totals is not None
                    spectral_totals["rust_err"] += spec["rust_err"]["total_spectral_energy"]
                    spectral_totals["c_err"] += spec["c_err"]["total_spectral_energy"]
                    spectral_totals["diff"] += spec["diff"]["total_spectral_energy"]
                    spectral_totals["full_windows"] += 1
            windows.append(entry)

        # Region aggregates group whole windows' raw energies (identical
        # formulas, larger spans).
        regions: list[dict[str, Any]] = []
        for region_index, (r0, r1) in enumerate(region_bounds):
            region_windows = [
                w
                for w in windows
                if w["orig_start_sample"] >= r0 and w["orig_end_sample"] <= r1
            ]
            # Raw-energy re-aggregation over the region span (same formulas).
            r_ref = r_err = 0.0
            c_ref = c_err = 0.0
            d_ref = d_err = 0.0
            r_peak = c_peak = d_peak = 0
            n = 0
            for w in region_windows:
                i0, i1 = w["orig_start_sample"], w["orig_end_sample"]
                o = orig_ch[i0:i1]
                r = rust_ch[lag_rust + i0:lag_rust + i1]
                cc = c_ch[lag_c + i0:lag_c + i1]
                for a, b in zip(o, r):
                    r_ref += float(a) * a
                    d = a - b
                    r_err += float(d) * d
                    r_peak = max(r_peak, abs(d))
                for a, b in zip(o, cc):
                    c_ref += float(a) * a
                    d = a - b
                    c_err += float(d) * d
                    c_peak = max(c_peak, abs(d))
                for a, b in zip(r, cc):
                    d_ref += float(a) * a
                    d = a - b
                    d_err += float(d) * d
                    d_peak = max(d_peak, abs(d))
                n += i1 - i0
            regions.append({
                "region_index": region_index,
                "orig_start_sample": r0,
                "orig_end_sample": r1,
                "valid_samples": r1 - r0,
                "start_time_s": round(r0 / rate, 6),
                "end_time_s": round(r1 / rate, 6),
                "window_indices": [w["window_index"] for w in region_windows],
                "rust_vs_original": aggregate_stats(
                    r_ref, r_err, n, r_peak,
                    max(orig_ch[r0:r1], key=abs) if r1 > r0 else 0,
                ),
                "c_vs_original": aggregate_stats(
                    c_ref, c_err, n, c_peak,
                    max(orig_ch[r0:r1], key=abs) if r1 > r0 else 0,
                ),
                "rust_vs_c": aggregate_stats(
                    d_ref, d_err, n, d_peak,
                    max(rust_ch[lag_rust + r0:lag_rust + r1], key=abs)
                    if r1 > r0 else 0,
                ),
            })

        # Joint full-band over exactly the windowed samples, then reconcile
        # the window aggregates against it (must match to rounding).
        full_rust = window_error_stats(
            orig_ch[0:joint_overlap],
            rust_ch[lag_rust:lag_rust + joint_overlap],
        )
        full_c = window_error_stats(
            orig_ch[0:joint_overlap],
            c_ch[lag_c:lag_c + joint_overlap],
        )
        full_diff = window_error_stats(
            rust_ch[lag_rust:lag_rust + joint_overlap],
            c_ch[lag_c:lag_c + joint_overlap],
        )
        window_agg = {
            "rust": aggregate_stats(
                acc["rust"]["ref"], acc["rust"]["err"], acc["rust"]["n"],
                acc["rust"]["peak"], acc["rust"]["sig_peak"],
            ),
            "c": aggregate_stats(
                acc["c"]["ref"], acc["c"]["err"], acc["c"]["n"],
                acc["c"]["peak"], acc["c"]["sig_peak"],
            ),
            "diff": aggregate_stats(
                acc["diff"]["ref"], acc["diff"]["err"], acc["diff"]["n"],
                acc["diff"]["peak"], acc["diff"]["sig_peak"],
            ),
        }

        def close(a: float | None, b: float | None, tol: float = 1e-6) -> bool:
            if a is None or b is None:
                return a is None and b is None
            return abs(a - b) <= tol

        reconciliation = {"window_count": len(windows), "checks": {}}
        for key, full in (
            ("rust_vs_original", full_rust),
            ("c_vs_original", full_c),
            ("rust_vs_c", full_diff),
        ):
            agg = window_agg["rust" if key == "rust_vs_original" else
                             "c" if key == "c_vs_original" else "diff"]
            full_rms = full["rms_error_lsb"]
            agg_rms = agg["rms_error_lsb"]
            reconciliation["checks"][key] = {
                "rms_matches": close(full_rms, agg_rms),
                "snr_matches": close(full["snr_db"], agg["snr_db"]),
                "peak_matches": full["peak_error_lsb"] == agg["peak_error_lsb"],
                "status_matches": full["snr_status"] == agg["snr_status"],
                "samples": full["samples_compared"],
            }
        reconciliation["reconciled"] = all(
            all(v for k, v in check.items() if k != "samples")
            for check in reconciliation["checks"].values()
        )

        # Compare against the B1 full-band numbers over the same samples.
        # B1 per-pair overlaps equal the joint overlap whenever both decodes
        # share length and lag (true for the whole retained corpus); any
        # residual difference is reported, not hidden.
        b1_overlap_note = None
        b1_deltas: dict[str, Any] = {}
        for key, b1_block in (
            ("rust_vs_original", b1_fixture["quality"]["rust_vs_original"]),
            ("c_vs_original", b1_fixture["quality"]["c_vs_original"]),
            ("rust_vs_c", b1_fixture["quality"]["rust_vs_c"]),
        ):
            b1_overlap = b1_block["overlap_samples_per_channel"]
            agg = window_agg["rust" if key == "rust_vs_original" else
                             "c" if key == "c_vs_original" else "diff"]
            if key == "rust_vs_c":
                b1_agg = b1_block["aggregate"]
                same_samples = b1_overlap == joint_overlap
                b1_deltas[key] = {
                    "b1_overlap": b1_overlap,
                    "joint_overlap": joint_overlap,
                    "same_samples": same_samples,
                    "b1_rms": b1_agg["rms_error_lsb"],
                    "b2_rms": agg["rms_error_lsb"],
                }
            else:
                b1_agg = b1_block["per_channel"][channel]
                same_samples = b1_overlap == joint_overlap
                b1_deltas[key] = {
                    "b1_overlap": b1_overlap,
                    "joint_overlap": joint_overlap,
                    "same_samples": same_samples,
                    "b1_rms": b1_agg["rms_error_lsb"],
                    "b1_snr": b1_agg["snr_db"],
                    "b2_rms": agg["rms_error_lsb"],
                    "b2_snr": agg["snr_db"],
                }
        if not all(v["same_samples"] for v in b1_deltas.values()):
            b1_overlap_note = (
                "rust_vs_original/c_vs_original cover the same samples as the "
                "B1 full-band; rust_vs_c differs by span: B1 compares the full "
                "decoded streams at lag 0 (including decoder priming never "
                "compared against the original), while B2 compares exactly "
                "the joint compared overlap"
            )

        channel_result: dict[str, Any] = {
            "channel": channel,
            "fullband_joint": {
                "rust_vs_original": {
                    k: full_rust[k] for k in (
                        "snr_db", "snr_status", "rms_error_lsb",
                        "peak_error_lsb", "signal_rms_lsb",
                        "signal_peak_lsb", "samples_compared")
                },
                "c_vs_original": {
                    k: full_c[k] for k in (
                        "snr_db", "snr_status", "rms_error_lsb",
                        "peak_error_lsb", "signal_rms_lsb",
                        "signal_peak_lsb", "samples_compared")
                },
                "rust_vs_c": {
                    "snr_db": full_diff["snr_db"],
                    "snr_status": full_diff["snr_status"],
                    "rms_difference_lsb": full_diff["rms_error_lsb"],
                    "peak_difference_lsb": full_diff["peak_error_lsb"],
                    "samples_compared": full_diff["samples_compared"],
                },
            },
            "reconciliation": reconciliation,
            "b1_comparison": {
                "same_samples": all(v["same_samples"] for v in b1_deltas.values()),
                "note": b1_overlap_note,
                "deltas": b1_deltas,
            },
            "windows": windows,
            "regions": regions,
        }
        if spectral_totals is not None:
            # Fixture/channel spectral totals = sums over full windows only.
            band_sums: list[float] = []
            band_labels: list[list[float]] = []
            n_bands = len(BAND_EDGES_HZ) - 1
            rust_bands = [0.0] * n_bands
            c_bands = [0.0] * n_bands
            diff_bands = [0.0] * n_bands
            for w in windows:
                spec = w.get("spectral")
                if not spec or spec.get("spectral_status") != "computed":
                    continue
                for i, band in enumerate(spec["rust_err"]["bands"]):
                    rust_bands[i] += band["energy"]
                for i, band in enumerate(spec["c_err"]["bands"]):
                    c_bands[i] += band["energy"]
                for i, band in enumerate(spec["diff"]["bands"]):
                    diff_bands[i] += band["energy"]
                if not band_labels:
                    band_labels = [band["band_hz"] for band in spec["rust_err"]["bands"]]
                    band_sums = [0.0] * len(band_labels)
            channel_result["spectral_totals"] = {
                "method": (
                    "Hann-windowed N-point DFT error energy per full analysis "
                    "window, summed over windows; not masking-weighted, not NMR"
                ),
                "full_windows": spectral_totals["full_windows"],
                "total_energy": {
                    "rust_err": spectral_totals["rust_err"],
                    "c_err": spectral_totals["c_err"],
                    "diff": spectral_totals["diff"],
                },
                "bands_hz": band_labels,
                "band_energy": {
                    "rust_err": rust_bands,
                    "c_err": c_bands,
                    "diff": diff_bands,
                },
                "band_bin_ranges": band_bin_ranges(window_samples, rate),
                "sample_rate_hz": rate,
            }
        per_channel.append(channel_result)

    # Tail / termination diagnostics from B1 length + tail fields.
    tail = build_tail_diagnostics(
        stem, b1_fixture, length_checks, per_channel,
        window_samples, joint_overlap, rate,
    )

    return {
        "stem": stem,
        "scope": b1_fixture["eligibility"]["scope"],
        "sample_rate_hz": rate,
        "channels": channels,
        "alignment": {
            "rust_selected_lag": lag_rust,
            "c_selected_lag": lag_c,
            "b1_rust_selected_lag": b1_lag_rust,
            "b1_c_selected_lag": b1_lag_c,
            "lags_consistent": True,
            "rust_ambiguity_status": align_rust["ambiguity_status"],
            "c_ambiguity_status": align_c["ambiguity_status"],
        },
        "decoded_length": {
            impl: {
                "frame_count": check["frame_count"],
                "expected_samples_per_channel": check["expected_samples_per_channel"],
                "actual_samples_per_channel": check["actual_samples_per_channel"],
                "delta_samples": check["delta_samples"],
                "status": check["status"],
            }
            for impl, check in length_checks.items()
        },
        "lengths_equal": (
            length_checks["rust"]["actual_samples_per_channel"]
            == length_checks["c"]["actual_samples_per_channel"]
        ),
        "joint_overlap_samples_per_channel": joint_overlap,
        "window_samples": window_samples,
        "window_count": len(window_bounds),
        "region_samples": region_samples,
        "region_count": len(region_bounds),
        "per_channel": per_channel,
        "tail": tail,
        "provenance": {
            "wave_source": triplet["paths"]["source_label"],
            "input_sha256_matches_manifest": True,
            "rust_wav_matches_b1_json": True,
            "c_wav_matches_b1_json": True,
            "v3_byte_match": triplet["v3_match"],
        },
    }


def build_tail_diagnostics(
    stem: str,
    b1_fixture: dict[str, Any],
    length_checks: dict[str, dict[str, Any]],
    per_channel: list[dict[str, Any]],
    window_samples: int,
    joint_overlap: int,
    rate: int,
) -> dict[str, Any]:
    quality = b1_fixture["quality"]
    b1_tails = {
        key: {
            "overlap": quality[key]["overlap_samples_per_channel"],
            "reference_tail": quality[key]["reference_tail_shortfall_samples_per_channel"],
            "test_tail": quality[key]["test_tail_shortfall_samples_per_channel"],
        }
        for key in ("rust_vs_original", "c_vs_original", "rust_vs_c")
    }
    window_count = len(per_channel[0]["windows"]) if per_channel else 0
    last_full_index = None
    partial_index = None
    for w in (per_channel[0]["windows"] if per_channel else []):
        if w["partial"]:
            partial_index = w["window_index"]
        else:
            last_full_index = w["window_index"]
    # Final-region differences: last up-to-4 windows (incl. partial tail).
    final_windows: list[dict[str, Any]] = []
    for channel_index, ch_result in enumerate(per_channel):
        for w in ch_result["windows"]:
            if window_count and w["window_index"] >= max(0, window_count - 4):
                final_windows.append({
                    "channel": ch_result.get("channel", channel_index),
                    "window_index": w["window_index"],
                    "partial": w["partial"],
                    "rust_rms": w["rust_vs_original"]["rms_error_lsb"],
                    "c_rms": w["c_vs_original"]["rms_error_lsb"],
                    "diff_rms": w["rust_vs_c"]["rms_difference_lsb"],
                    "diff_peak": w["rust_vs_c"]["peak_difference_lsb"],
                })
    max_final_diff = max((w["diff_rms"] or 0.0) for w in final_windows) \
        if final_windows else 0.0
    lengths_equal = (
        length_checks["rust"]["actual_samples_per_channel"]
        == length_checks["c"]["actual_samples_per_channel"]
    )
    if lengths_equal:
        interpretation = (
            "Rust/C decoded lengths are equal, so any final-region Rust-vs-C "
            "difference persists within the overlapping samples; it is not an "
            "aggregate alignment/length effect."
            if max_final_diff > 0 else
            "Rust/C decoded lengths are equal and the final region shows no "
            "Rust-vs-C difference."
        )
    else:
        interpretation = (
            "Rust/C decoded lengths differ; a final-region difference may be "
            "an aggregate alignment/length effect. Within-overlap windows "
            "above isolate the overlapping-sample contribution."
        )
    return {
        "expected_decoded_length": {
            impl: length_checks[impl]["expected_samples_per_channel"]
            for impl in ("rust", "c")
        },
        "actual_decoded_length": {
            impl: length_checks[impl]["actual_samples_per_channel"]
            for impl in ("rust", "c")
        },
        "shortfall_delta": {
            impl: length_checks[impl]["delta_samples"] for impl in ("rust", "c")
        },
        "b1_tail_shortfalls": b1_tails,
        "last_complete_window_index": last_full_index,
        "partial_tail_window_index": partial_index,
        "partial_tail_valid_samples": (
            joint_overlap - last_full_index * window_samples - window_samples
            if last_full_index is not None and partial_index is not None
            else (joint_overlap if partial_index == 0 else 0)
        ),
        "final_windows": final_windows,
        "max_final_diff_rms_lsb": round(max_final_diff, 6),
        "lengths_equal": lengths_equal,
        "interpretation": interpretation,
    }


# --------------------------------------------------------------------------
# Report rendering
# --------------------------------------------------------------------------

def render_report(result: dict[str, Any]) -> str:
    lines: list[str] = []
    lines.append("# AAC-LC VBR-5 decoded-error localization (F04-B2)")
    lines.append("")
    lines.append(
        "Diagnostic measurement only. No perceptual, ABX, or listening claim. "
        "No pass/fail thresholds. No single quality score. A decoded analysis "
        "window is not an AAC encoder frame."
    )
    lines.append("")
    lines.append("## Run")
    lines.append("")
    run = result["run"]
    lines.append(f"- B2 tool: `{run['tool']} {run['tool_version']}`")
    lines.append(f"- B1 JSON: `{run['b1_json_label']}` "
                 f"(tool `{run['b1_tool']}`, manifest `{run['b1_manifest'][:16]}..`)")
    lines.append(f"- Wave source: `{run['wave_source']}`")
    lines.append(f"- Window: `{run['window_samples']} samples/channel` "
                 f"({'AAC frame-sized' if run['window_samples'] == 1024 else 'NON-STANDARD size'})")
    lines.append(f"- Region: `{run['region_samples']} samples/channel`")
    lines.append(f"- Spectral mode: `{run['spectral_mode']}`")
    lines.append(f"- Fixtures analyzed: `{run['analyzed_count']}` "
                 f"({', '.join(run['analyzed_stems']) if run['analyzed_stems'] else 'none'})")
    if run["skipped"]:
        lines.append("- Skipped (B1-excluded, passed through): "
                     + ", ".join(f"`{s['stem']}` ({s['reason']})" for s in run["skipped"]))
    lines.append("")
    lines.append("## Frame-to-time mapping and windowing")
    lines.append("")
    lines.append(
        "Window `k` covers original samples `[k*W, min((k+1)*W, overlap))`; "
        "time in seconds is `sample / rate`. Both implementations are compared "
        "against the same original samples; Rust-vs-C compares the decoded "
        "samples aligned to that interval. The first window starts at original "
        "sample 0; the last window is partial when `overlap % W != 0` and is "
        "flagged. Metrics on partial windows use only valid samples. Decoded "
        "windows must not be read as encoder frames: encoder delay, MDCT "
        "overlap-add, and decoder priming prevent a sample-to-frame causal "
        "mapping (as stated in the B1 manifest)."
    )
    lines.append("")
    for fixture in result["fixtures"]:
        lines.append(f"## {fixture['stem']} (`{fixture['scope']}`)")
        lines.append("")
        lines.append(
            f"- Rate/channels: `{fixture['sample_rate_hz']}/{fixture['channels']}`; "
            f"lags R/C: `{fixture['alignment']['rust_selected_lag']}/"
            f"{fixture['alignment']['c_selected_lag']}` "
            f"(consistent with B1: `{fixture['alignment']['lags_consistent']}`)"
        )
        lines.append(
            f"- Joint overlap: `{fixture['joint_overlap_samples_per_channel']}` "
            f"samples/ch in `{fixture['window_count']}` windows, "
            f"`{fixture['region_count']}` regions"
        )
        lines.append(
            f"- Decoded length R/C: "
            f"`{fixture['decoded_length']['rust']['actual_samples_per_channel']}/"
            f"{fixture['decoded_length']['c']['actual_samples_per_channel']}` "
            f"(expected "
            f"`{fixture['decoded_length']['rust']['expected_samples_per_channel']}/"
            f"{fixture['decoded_length']['c']['expected_samples_per_channel']}`; "
            f"equal: `{fixture['lengths_equal']}`)"
        )
        for ch in fixture["per_channel"]:
            lines.append(f"### Channel {ch['channel']}")
            lines.append("")
            fb = ch["fullband_joint"]
            lines.append(
                f"- Joint full-band RMS R/C: "
                f"`{fb['rust_vs_original']['rms_error_lsb']}/"
                f"{fb['c_vs_original']['rms_error_lsb']}` LSB; "
                f"diff RMS/peak: `{fb['rust_vs_c']['rms_difference_lsb']}/"
                f"{fb['rust_vs_c']['peak_difference_lsb']}`"
            )
            rec = ch["reconciliation"]
            lines.append(
                f"- Window aggregates reconcile with joint full-band: "
                f"`{rec['reconciled']}`"
            )
            b1c = ch["b1_comparison"]
            same = {k: v["same_samples"] for k, v in b1c["deltas"].items()}
            lines.append(
                f"- Same samples as B1 full-band: rust-vs-original "
                f"`{same['rust_vs_original']}`, c-vs-original "
                f"`{same['c_vs_original']}`, rust-vs-c `{same['rust_vs_c']}`"
                + (f" ({b1c['note']})" if b1c["note"] else "")
            )
            # Top windows by Rust-vs-C difference.
            ranked = sorted(
                ch["windows"],
                key=lambda w: (w["rust_vs_c"]["rms_difference_lsb"] or 0.0),
                reverse=True,
            )
            lines.append("- Top 5 windows by Rust-vs-C diff RMS:")
            for w in ranked[:5]:
                lines.append(
                    f"  - w{w['window_index']} "
                    f"[{w['start_time_s']:.4f}s, {w['end_time_s']:.4f}s]"
                    f"{' (partial)' if w['partial'] else ''}: "
                    f"diff RMS `{w['rust_vs_c']['rms_difference_lsb']}`, "
                    f"peak `{w['rust_vs_c']['peak_difference_lsb']}`, "
                    f"R RMS `{w['rust_vs_original']['rms_error_lsb']}`, "
                    f"C RMS `{w['c_vs_original']['rms_error_lsb']}`"
                )
            # Peak region.
            top_region = max(
                ch["regions"],
                key=lambda r: next(
                    (x for x in [r["rust_vs_c"].get("rms_error_lsb")] if x is not None),
                    0.0,
                ),
            ) if ch["regions"] else None
            if top_region is not None:
                lines.append(
                    f"- Peak diff region: r{top_region['region_index']} "
                    f"[{top_region['start_time_s']:.4f}s, {top_region['end_time_s']:.4f}s], "
                    f"windows {top_region['window_indices']}"
                )
            if "spectral_totals" in ch:
                st = ch["spectral_totals"]
                lines.append(
                    "- Spectral totals (Hann DFT energy, full windows only, "
                    "not masking-weighted):"
                )
                for i, hz in enumerate(st["bands_hz"]):
                    lines.append(
                        f"  - {hz[0]:.0f}-{hz[1]:.0f} Hz: rust-err "
                        f"`{st['band_energy']['rust_err'][i]:.3f}`, c-err "
                        f"`{st['band_energy']['c_err'][i]:.3f}`, diff "
                        f"`{st['band_energy']['diff'][i]:.3f}`"
                    )
            lines.append("")
        tail = fixture["tail"]
        lines.append("### Tail / termination")
        lines.append("")
        lines.append(
            f"- Expected/actual decoded length R: "
            f"`{tail['expected_decoded_length']['rust']}/"
            f"{tail['actual_decoded_length']['rust']}`; C: "
            f"`{tail['expected_decoded_length']['c']}/"
            f"{tail['actual_decoded_length']['c']}`"
        )
        lines.append(
            f"- Last complete window: `{tail['last_complete_window_index']}`; "
            f"partial tail window: `{tail['partial_tail_window_index']}` "
            f"(`{tail['partial_tail_valid_samples']}` valid samples)"
        )
        lines.append(
            f"- Max final-region diff RMS: `{tail['max_final_diff_rms_lsb']}` LSB"
        )
        lines.append(f"- {tail['interpretation']}")
        lines.append("")
    lines.append("## What this does not establish")
    lines.append("")
    lines.append(
        "- No perceptual equivalence, audibility, transparency, or quality "
        "ranking. Window/region/band differences are error-energy "
        "descriptions, not listening evidence."
    )
    lines.append(
        "- No causal attribution: a hot window or band does not identify a "
        "codec stage or defect. Correlation with transients, silence "
        "boundaries, or termination is descriptive, not causal."
    )
    lines.append(
        "- Spectral band energies are windowed DFT error energies with "
        "identical analysis for all signals. They are not NMR, not "
        "masking-weighted, and must not be read as audibility."
    )
    lines.append(
        "- Tail observations use B1's decoder-delay accounting unchanged; a "
        "tail difference is not labelled a codec defect."
    )
    lines.append("")
    return "\n".join(lines)


# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="AAC-LC VBR-5 decoded-error localization (diagnostic only)"
    )
    parser.add_argument("--manifest", type=Path,
                        default=HERE / "fixtures-v3.json")
    parser.add_argument("--b1-json", type=Path, required=True,
                        help="B1 metrics.json with eligibility/alignment/quality")
    parser.add_argument("--b1-work-dir", type=Path,
                        help="B1 work dir (inputs/<stem>.wav, decoded/<stem>.{rust,c}.wav)")
    parser.add_argument("--v3-reference-dir", type=Path,
                        help="retained V3 root (inputs/, rust-wav/, c-wav/); "
                             "wave source when --b1-work-dir is absent, else "
                             "byte-for-byte cross-check")
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--only", help="comma-separated stems (subset of B1 JSON)")
    parser.add_argument("--window-samples", type=int, default=DEFAULT_WINDOW_SAMPLES)
    parser.add_argument("--region-samples", type=int, default=DEFAULT_REGION_SAMPLES)
    parser.add_argument("--enable-spectral", action="store_true")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args(argv)

    try:
        if args.window_samples <= 0:
            raise LocalizeError("--window-samples must be positive")
        if args.region_samples <= 0:
            raise LocalizeError("--region-samples must be positive")
        if args.b1_work_dir is None and args.v3_reference_dir is None:
            raise LocalizeError(
                "pass --b1-work-dir and/or --v3-reference-dir "
                "(both verifies workdir WAVs byte-for-byte against retained V3)"
            )
        if args.enable_spectral and not is_power_of_two(args.window_samples):
            raise LocalizeError(
                "--enable-spectral needs a power-of-two --window-samples"
            )
        manifest = b1.load_json(args.manifest.resolve())
        b1.validate_manifest(manifest)
        manifest_by_stem = {f["stem"]: f for f in manifest["fixtures"]}
        b1_data = b1.load_json(args.b1_json.resolve())
        b1_by_stem = {f["stem"]: f for f in b1_data["fixtures"]}
        if args.only:
            wanted = [p.strip() for p in args.only.split(",") if p.strip()]
            if not wanted:
                raise LocalizeError("--only was provided but contained no stems")
            unknown = [s for s in wanted if s not in b1_by_stem]
            if unknown:
                raise LocalizeError(f"--only contains stems absent from B1 JSON: {unknown}")
            stems = wanted
        else:
            stems = [f["stem"] for f in b1_data["fixtures"]]

        out_dir = args.out_dir.resolve()
        out_dir.mkdir(parents=True, exist_ok=True)
        b1_work_dir = args.b1_work_dir.resolve() if args.b1_work_dir else None
        v3_dir = args.v3_reference_dir.resolve() if args.v3_reference_dir else None

        alignment_contract = manifest["alignment"]
        decoder_delays = manifest["decoder_output_delay_samples"]
        frame_length = int(manifest["encoder_configuration"]["frame_length_samples"])

        fixtures: list[dict[str, Any]] = []
        skipped: list[dict[str, str]] = []
        for stem in stems:
            manifest_fixture = manifest_by_stem.get(stem)
            b1_fixture = b1_by_stem[stem]
            if manifest_fixture is None or not manifest_fixture.get("corpus_member"):
                skipped.append({"stem": stem, "reason": "not a corpus member"})
                continue
            if b1_fixture.get("quality") is None:
                reasons = "; ".join(
                    b1_fixture.get("eligibility", {}).get("reasons", [])
                ) or b1_fixture.get("eligibility", {}).get("scope", "excluded")
                skipped.append({"stem": stem, "reason": f"B1-excluded: {reasons}"})
                continue
            triplet = load_triplet(
                stem, manifest_fixture, b1_fixture,
                b1_work_dir=b1_work_dir, v3_reference_dir=v3_dir,
            )
            fixtures.append(localize_fixture(
                manifest_fixture, b1_fixture, triplet,
                alignment_contract, decoder_delays, frame_length,
                window_samples=args.window_samples,
                region_samples=args.region_samples,
                enable_spectral=args.enable_spectral,
            ))

        result = {
            "schema_version": 1,
            "tool": {"name": TOOL_NAME, "version": TOOL_VERSION},
            "run": {
                "tool": TOOL_NAME,
                "tool_version": TOOL_VERSION,
                "b1_json_label": "b1-metrics-json",
                "b1_tool": b1_data.get("tool", {}),
                "b1_manifest": b1_data.get("manifest", {}).get("sha256"),
                "wave_source": (
                    "b1-work-dir+v3-cross-check"
                    if (b1_work_dir is not None and v3_dir is not None)
                    else "b1-work-dir"
                    if b1_work_dir is not None else "retained-v3"
                ),
                "window_samples": args.window_samples,
                "region_samples": args.region_samples,
                "spectral_mode": (
                    "hann-dft-bands (diagnostic, not masking-weighted)"
                    if args.enable_spectral else "off"
                ),
                "analyzed_count": len(fixtures),
                "analyzed_stems": [f["stem"] for f in fixtures],
                "skipped": skipped,
            },
            "interpretation": {
                "perceptual_claim": False,
                "thresholds_used": False,
                "single_quality_score": False,
                "decoder_mapping": (
                    "analysis windowing only; no AAC-frame-to-sample causal mapping"
                ),
                "spectral_label": (
                    "windowed DFT error energy; not NMR; no masking model"
                    if args.enable_spectral else "not computed"
                ),
            },
            "fixtures": fixtures,
            "skipped": skipped,
        }
        json_path = args.json.resolve() if args.json else out_dir / "localization.json"
        report_path = args.report.resolve() if args.report else out_dir / "report.md"
        json_path.parent.mkdir(parents=True, exist_ok=True)
        report_path.parent.mkdir(parents=True, exist_ok=True)
        json_path.write_text(
            json.dumps(result, indent=2, sort_keys=False) + "\n", encoding="utf-8"
        )
        report_path.write_text(render_report(result), encoding="utf-8")
        print(f"wrote report: {report_path}")
        print(f"wrote JSON:   {json_path}")
        print(f"analyzed={len(fixtures)} skipped={len(skipped)}")
        return 0
    except (LocalizeError, b1.MetricsError) as exc:
        print(f"{TOOL_NAME}: error: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
