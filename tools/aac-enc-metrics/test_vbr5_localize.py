#!/usr/bin/env python3
"""Focused tests for tools/aac-enc-metrics/vbr5_localize.py (F04-B2).

Synthetic only: no AAC encoding/decoding. Covers identical streams, a known
error confined to one window, channel-specific differences, partial
first/last windows, silence/zero-error handling, invalid rates/channels/
lengths, deterministic output, and spectral band accounting/energy
conservation.
"""

from __future__ import annotations

import cmath
import json
import math
import struct
import sys
import tempfile
import unittest
import wave
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import vbr5_metrics as b1  # noqa: E402
import vbr5_localize as loc  # noqa: E402


def write_wav(path: Path, channels: list[list[int]], sample_rate: int = 48000) -> None:
    frame_count = len(channels[0])
    for channel in channels:
        if len(channel) != frame_count:
            raise AssertionError("channel lengths differ")
    raw = bytearray()
    for i in range(frame_count):
        for channel in channels:
            raw += struct.pack("<h", channel[i])
    with wave.open(str(path), "wb") as w:
        w.setnchannels(len(channels))
        w.setsampwidth(2)
        w.setframerate(sample_rate)
        w.writeframes(bytes(raw))


def impulse(length: int = 6000, index: int = 2000) -> list[int]:
    signal = [0] * length
    signal[index] = 30000
    return signal


def pcm_wav(channels: list[list[int]], rate: int = 48000) -> dict:
    return {
        "sample_rate_hz": rate,
        "channels": len(channels),
        "samples_per_channel": len(channels[0]),
        "bits_per_sample": 16,
        "pcm": channels,
    }


def contract() -> dict:
    return {
        "nominal_lag_samples": 1600,
        "search_half_width_samples": 32,
        "channel_tolerance_samples": 1,
        "nominal_tolerance_samples": 1,
        "tie_epsilon": 1e-9,
        "coarse_step_samples": 8,
        "short_signal_threshold_samples": 48000,
        "short_signal_coarse_step_samples": 1,
        "refine_half_width_samples": 2,
        "long_signal_refine_half_width_samples": 4,
    }


def make_triplet(
    original: list[int],
    rust_err: list[int] | None = None,
    c_err: list[int] | None = None,
    *,
    channels: int = 1,
    rate: int = 48000,
    lag: int = 1600,
    rust_ch1_err: list[int] | None = None,
) -> dict:
    """Build an in-memory triplet; decoded = lag zeros + original + error."""
    orig_chs = [list(original) for _ in range(channels)]
    rust_chs, c_chs = [], []
    for ch in range(channels):
        r = [0] * lag + list(original)
        c = [0] * lag + list(original)
        err = rust_err if not (ch == 1 and rust_ch1_err is not None) else rust_ch1_err
        if err is not None:
            r = [0] * lag + [a + b for a, b in zip(original, err)]
        if c_err is not None:
            c = [0] * lag + [a + b for a, b in zip(original, c_err)]
        rust_chs.append(r)
        c_chs.append(c)
    return {
        "original": pcm_wav(orig_chs, rate),
        "rust": pcm_wav(rust_chs, rate),
        "c": pcm_wav(c_chs, rate),
        "paths": {"source_label": "test"},
        "rust_sha256": "test",
        "c_sha256": "test",
        "v3_match": None,
    }


def make_b1_fixture(triplet: dict, scope: str = "gated") -> dict:
    original, rust, c = triplet["original"], triplet["rust"], triplet["c"]
    return {
        "eligibility": {"scope": scope},
        "alignment": {
            "rust": {"selected_lag_samples": 1600},
            "c": {"selected_lag_samples": 1600},
        },
        "encoded": {"rust": {"frame_count": 9}, "c": {"frame_count": 9}},
        "quality": {
            "rust_vs_original": b1.measure_pair(
                original, rust, lag_samples=1600,
                reference_name="original", test_name="rust_decode"),
            "c_vs_original": b1.measure_pair(
                original, c, lag_samples=1600,
                reference_name="original", test_name="c_decode"),
            "rust_vs_c": b1.measure_pair(
                rust, c, lag_samples=0,
                reference_name="rust_decode", test_name="c_decode"),
        },
        "decoded": {
            "rust": {"wav_sha256": "test"},
            "c": {"wav_sha256": "test"},
        },
    }


def make_manifest_fixture(length: int, channels: int = 1,
                           rate: int = 48000) -> dict:
    return {
        "stem": "synthetic",
        "corpus_member": True,
        "sample_rate_hz": rate,
        "channels": channels,
        "samples_per_channel": length,
        "input_sha256": "test",
    }


class WindowStatsTests(unittest.TestCase):
    def test_identical_decoded_streams(self) -> None:
        original = impulse()
        triplet = make_triplet(original)
        result = loc.localize_fixture(
            make_manifest_fixture(len(original)), make_b1_fixture(triplet),
            triplet, contract(), {"48000": 1744, "44100": 1685}, 1024,
            window_samples=1024, region_samples=4096, enable_spectral=False)
        ch = result["per_channel"][0]
        # The impulse sits in window 1; all other windows are digital silence.
        for w in ch["windows"]:
            if w["window_index"] == 1:
                self.assertEqual(w["rust_vs_original"]["snr_status"], "identical")
            else:
                # Zero reference energy: B1 reports undefined_silence even
                # when the error is also zero.
                self.assertEqual(
                    w["rust_vs_original"]["snr_status"], "undefined_silence")
                self.assertIsNone(w["rust_vs_original"]["snr_db"])
            self.assertEqual(w["rust_vs_c"]["rms_difference_lsb"], 0.0)
            self.assertEqual(w["rust_vs_c"]["peak_difference_lsb"], 0)
        self.assertTrue(ch["reconciliation"]["reconciled"])
        self.assertEqual(result["tail"]["max_final_diff_rms_lsb"], 0.0)

    def test_known_error_confined_to_one_window(self) -> None:
        # 6000-sample overlap -> windows of 1024: w5 covers [5120, 6000).
        original = impulse()
        err = [0] * len(original)
        for i in range(5120, 6000):
            err[i] = 100
        triplet = make_triplet(original, rust_err=err)
        result = loc.localize_fixture(
            make_manifest_fixture(len(original)), make_b1_fixture(triplet),
            triplet, contract(), {"48000": 1744, "44100": 1685}, 1024,
            window_samples=1024, region_samples=4096, enable_spectral=False)
        ch = result["per_channel"][0]
        hot = [w for w in ch["windows"] if (w["rust_vs_c"]["rms_difference_lsb"] or 0) > 0]
        self.assertEqual([w["window_index"] for w in hot], [5])
        self.assertEqual(hot[0]["partial"], True)
        self.assertEqual(hot[0]["valid_samples"], 6000 - 5120)
        # Rust error is localized too; C stays clean everywhere.
        for w in ch["windows"]:
            self.assertEqual(w["c_vs_original"]["rms_error_lsb"], 0.0)
        self.assertGreater(hot[0]["rust_vs_original"]["rms_error_lsb"], 0.0)
        self.assertTrue(ch["reconciliation"]["reconciled"])

    def test_channel_specific_difference(self) -> None:
        original = impulse()
        err_ch1 = [50] * len(original)
        triplet = make_triplet(original, channels=2, rust_ch1_err=err_ch1)
        b1f = make_b1_fixture(triplet)
        result = loc.localize_fixture(
            make_manifest_fixture(len(original), channels=2),
            b1f, triplet, contract(),
            {"48000": 1744, "44100": 1685}, 1024,
            window_samples=1024, region_samples=4096, enable_spectral=False)
        ch0_max = max(w["rust_vs_c"]["rms_difference_lsb"] or 0.0
                      for w in result["per_channel"][0]["windows"])
        ch1_max = max(w["rust_vs_c"]["rms_difference_lsb"] or 0.0
                      for w in result["per_channel"][1]["windows"])
        self.assertEqual(ch0_max, 0.0)
        self.assertGreater(ch1_max, 0.0)

    def test_partial_first_and_last_windows(self) -> None:
        # Overlap (1000) shorter than one window but at B1's minimum scoring
        # overlap: a single partial window that is both first and last.
        original = impulse(length=1000, index=500)
        triplet = make_triplet(original)
        b1f = make_b1_fixture(triplet)
        result = loc.localize_fixture(
            make_manifest_fixture(len(original)), b1f, triplet, contract(),
            {"48000": 1744, "44100": 1685}, 1024,
            window_samples=1024, region_samples=4096, enable_spectral=False)
        windows = result["per_channel"][0]["windows"]
        self.assertEqual(len(windows), 1)
        self.assertTrue(windows[0]["partial"])
        self.assertEqual(windows[0]["valid_samples"], 1000)
        self.assertEqual(windows[0]["orig_start_sample"], 0)
        self.assertEqual(windows[0]["orig_end_sample"], 1000)

    def test_silence_and_zero_error_statuses(self) -> None:
        silent = [0] * 3000
        # Zero reference energy -> undefined_silence, even with zero error
        # (B1 checks reference energy first).
        stats = loc.window_error_stats(silent, list(silent))
        self.assertEqual(stats["snr_status"], "undefined_silence")
        self.assertIsNone(stats["snr_db"])
        self.assertEqual(stats["rms_error_lsb"], 0.0)
        # Non-zero reference with zero error -> identical.
        nonzero = impulse(length=3000)
        stats = loc.window_error_stats(nonzero, list(nonzero))
        self.assertEqual(stats["snr_status"], "identical")
        # Zero reference with nonzero test -> undefined_silence.
        stats = loc.window_error_stats(silent, [1] * 3000)
        self.assertEqual(stats["snr_status"], "undefined_silence")
        self.assertIsNone(stats["snr_db"])
        # Empty slice -> insufficient overlap.
        stats = loc.window_error_stats([], [])
        self.assertEqual(stats["snr_status"], "insufficient_overlap")
        self.assertEqual(stats["samples_compared"], 0)

    def test_invalid_rates_channels_lengths(self) -> None:
        original = impulse()
        triplet = make_triplet(original)
        manifest = make_manifest_fixture(len(original))
        b1f = make_b1_fixture(triplet)
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "inputs").mkdir(parents=True)
            (root / "rust-wav").mkdir(parents=True)
            (root / "c-wav").mkdir(parents=True)
            write_wav(root / "inputs" / "s.wav", [original])
            # Decoded rust WAV at the wrong sample rate.
            write_wav(root / "rust-wav" / "s.rust.wav",
                      [triplet["rust"]["pcm"][0]], sample_rate=44100)
            write_wav(root / "c-wav" / "s.c.wav", [triplet["c"]["pcm"][0]])
            bad_b1 = json.loads(json.dumps(b1f))
            with self.assertRaises(loc.LocalizeError):
                loc.load_triplet("s", manifest, bad_b1, b1_work_dir=None,
                                 v3_reference_dir=root)
        # Channel-count mismatch vs the manifest is caught at load time.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "inputs").mkdir(parents=True)
            (root / "rust-wav").mkdir(parents=True)
            (root / "c-wav").mkdir(parents=True)
            write_wav(root / "inputs" / "s.wav", [original])
            write_wav(root / "rust-wav" / "s.rust.wav",
                      [triplet["rust"]["pcm"][0], triplet["rust"]["pcm"][0]])
            write_wav(root / "c-wav" / "s.c.wav", [triplet["c"]["pcm"][0]])
            bad_b1 = json.loads(json.dumps(b1f))
            with self.assertRaises(loc.LocalizeError):
                loc.load_triplet("s", manifest, bad_b1, b1_work_dir=None,
                                 v3_reference_dir=root)
        # Mismatched window slice lengths.
        with self.assertRaises(loc.LocalizeError):
            loc.window_error_stats([1, 2, 3], [1, 2])

    def test_region_summary_groups_windows(self) -> None:
        original = impulse()
        triplet = make_triplet(original)
        result = loc.localize_fixture(
            make_manifest_fixture(len(original)), make_b1_fixture(triplet),
            triplet, contract(), {"48000": 1744, "44100": 1685}, 1024,
            window_samples=1024, region_samples=4096, enable_spectral=False)
        regions = result["per_channel"][0]["regions"]
        self.assertEqual(len(regions), 2)  # [0,4096), [4096,6000)
        self.assertEqual(regions[0]["window_indices"], [0, 1, 2, 3])
        self.assertEqual(regions[1]["window_indices"], [4, 5])
        self.assertEqual(regions[1]["valid_samples"], 6000 - 4096)


class SpectralTests(unittest.TestCase):
    def test_fft_matches_naive_dft(self) -> None:
        values = [complex(n * (-1) ** n, n) for n in range(8)]
        got = loc.fft_forward(values)
        for k in range(8):
            expected = sum(v * cmath.exp(complex(0, -2 * math.pi * k * n / 8))
                           for n, v in enumerate(values))
            self.assertAlmostEqual(got[k].real, expected.real, places=9)
            self.assertAlmostEqual(got[k].imag, expected.imag, places=9)

    def test_fft_rejects_non_power_of_two(self) -> None:
        with self.assertRaises(loc.LocalizeError):
            loc.fft_forward([complex(1)] * 6)

    def test_band_accounting_and_energy_conservation(self) -> None:
        n = 1024
        window = loc.hann_window(n)
        error = [int(round(500 * math.sin(2 * math.pi * 440 * i / 48000)))
                 for i in range(n)]
        spec = loc.spectral_band_energies(error, window, 48000)
        bands = spec["bands"]
        # Band energies sum to the spectral total (float summation order).
        band_sum = sum(b["energy"] for b in bands)
        self.assertLess(abs(band_sum - spec["total_spectral_energy"])
                        / spec["total_spectral_energy"], 1e-9)
        # Parseval: total == N * time_energy (tight for float64).
        self.assertLess(spec["parseval_relative_error"], 1e-9)
        # 440 Hz tone falls in the 0-1000 Hz band.
        self.assertEqual(bands[0]["band_hz"], [0.0, 1000.0])
        loudest = max(range(len(bands)), key=lambda i: bands[i]["energy"])
        self.assertEqual(loudest, 0)
        # Bins tile DC..Nyquist exactly once.
        ranges = loc.band_bin_ranges(n, 48000)
        self.assertEqual([(r["bin_lo"], r["bin_hi"]) for r in ranges],
                         [(0, 21), (22, 85), (86, 170), (171, 255), (256, 512)])
        covered = sum(r["bin_hi"] - r["bin_lo"] + 1 for r in ranges)
        self.assertEqual(covered, n // 2 + 1)

    def test_spectral_silence_is_zero(self) -> None:
        n = 1024
        spec = loc.spectral_band_energies([0] * n, loc.hann_window(n), 48000)
        self.assertEqual(spec["total_spectral_energy"], 0.0)
        self.assertTrue(all(b["energy"] == 0.0 for b in spec["bands"]))

    def test_spectral_partial_windows_skipped(self) -> None:
        original = impulse()
        triplet = make_triplet(original)
        result = loc.localize_fixture(
            make_manifest_fixture(len(original)), make_b1_fixture(triplet),
            triplet, contract(), {"48000": 1744, "44100": 1685}, 1024,
            window_samples=1024, region_samples=4096, enable_spectral=True)
        windows = result["per_channel"][0]["windows"]
        full = [w for w in windows if not w["partial"]]
        partial = [w for w in windows if w["partial"]]
        self.assertTrue(full)
        self.assertTrue(partial)
        for w in full:
            self.assertEqual(w["spectral"]["spectral_status"], "computed")
        for w in partial:
            self.assertEqual(w["spectral"]["spectral_status"], "partial_skipped")
        totals = result["per_channel"][0]["spectral_totals"]
        self.assertEqual(totals["full_windows"], len(full))
        self.assertIn("not masking-weighted", totals["method"])


class TailTests(unittest.TestCase):
    def test_tail_equal_lengths_persists_within_overlap(self) -> None:
        per_channel = [{
            "windows": [
                {"window_index": 0, "partial": False,
                 "rust_vs_original": {"rms_error_lsb": 1.0},
                 "c_vs_original": {"rms_error_lsb": 1.0},
                 "rust_vs_c": {"rms_difference_lsb": 0.5,
                               "peak_difference_lsb": 2}},
                {"window_index": 1, "partial": True,
                 "rust_vs_original": {"rms_error_lsb": 1.0},
                 "c_vs_original": {"rms_error_lsb": 1.0},
                 "rust_vs_c": {"rms_difference_lsb": 0.0,
                               "peak_difference_lsb": 0}},
            ],
        }]
        checks = {
            "rust": {"frame_count": 9, "expected_samples_per_channel": 7472,
                     "actual_samples_per_channel": 7600, "delta_samples": 128,
                     "status": "implausible"},
            "c": {"frame_count": 9, "expected_samples_per_channel": 7472,
                  "actual_samples_per_channel": 7600, "delta_samples": 128,
                  "status": "implausible"},
        }
        b1f = {"quality": {
            "rust_vs_original": {"overlap_samples_per_channel": 6000,
                                 "reference_tail_shortfall_samples_per_channel": 0,
                                 "test_tail_shortfall_samples_per_channel": 1600},
            "c_vs_original": {"overlap_samples_per_channel": 6000,
                              "reference_tail_shortfall_samples_per_channel": 0,
                              "test_tail_shortfall_samples_per_channel": 1600},
            "rust_vs_c": {"overlap_samples_per_channel": 7600,
                          "reference_tail_shortfall_samples_per_channel": 0,
                          "test_tail_shortfall_samples_per_channel": 0},
        }}
        tail = loc.build_tail_diagnostics(
            "s", b1f, checks, per_channel, 1024, 6000, 48000)
        self.assertTrue(tail["lengths_equal"])
        self.assertIn("within the overlapping samples", tail["interpretation"])
        self.assertEqual(tail["last_complete_window_index"], 0)
        self.assertEqual(tail["partial_tail_window_index"], 1)

    def test_tail_unequal_lengths_warns_length_effect(self) -> None:
        per_channel = [{"windows": []}]
        checks = {
            "rust": {"frame_count": 9, "expected_samples_per_channel": 7472,
                     "actual_samples_per_channel": 7600, "delta_samples": 128,
                     "status": "implausible"},
            "c": {"frame_count": 9, "expected_samples_per_channel": 7472,
                  "actual_samples_per_channel": 7599, "delta_samples": 127,
                  "status": "implausible"},
        }
        b1f = {"quality": {
            "rust_vs_original": {"overlap_samples_per_channel": 6000,
                                 "reference_tail_shortfall_samples_per_channel": 0,
                                 "test_tail_shortfall_samples_per_channel": 1600},
            "c_vs_original": {"overlap_samples_per_channel": 5999,
                              "reference_tail_shortfall_samples_per_channel": 1,
                              "test_tail_shortfall_samples_per_channel": 1600},
            "rust_vs_c": {"overlap_samples_per_channel": 7599,
                          "reference_tail_shortfall_samples_per_channel": 1,
                          "test_tail_shortfall_samples_per_channel": 0},
        }}
        tail = loc.build_tail_diagnostics(
            "s", b1f, checks, per_channel, 1024, 5999, 48000)
        self.assertFalse(tail["lengths_equal"])
        self.assertIn("alignment/length effect", tail["interpretation"])


class EndToEndTests(unittest.TestCase):
    def setUp(self) -> None:
        # Synthetic single-fixture manifests cannot satisfy B1's 19-member
        # corpus shape check; bypass only that shape check while keeping every
        # reused B1 measurement function (alignment, length, error stats,
        # WAV/ADTS parsing) intact.
        import unittest.mock as mock
        self._patcher = mock.patch.object(
            b1, "validate_manifest", lambda manifest: None)
        self._patcher.start()

    def tearDown(self) -> None:
        self._patcher.stop()

    def write_case(self, root: Path, stem: str, original: list[int],
                   rust_err: list[int] | None = None) -> None:
        triplet = make_triplet(original, rust_err=rust_err)
        (root / "inputs").mkdir(parents=True, exist_ok=True)
        (root / "decoded").mkdir(parents=True, exist_ok=True)
        write_wav(root / "inputs" / f"{stem}.wav", [original])
        write_wav(root / "decoded" / f"{stem}.rust.wav",
                  triplet["rust"]["pcm"])
        write_wav(root / "decoded" / f"{stem}.c.wav", triplet["c"]["pcm"])
        return triplet

    def b1_json(self, root: Path, stems: list[str],
                triplets: dict[str, dict]) -> Path:
        import hashlib
        fixtures = []
        for stem in stems:
            triplet = triplets[stem]
            original = triplet["original"]
            rust = triplet["rust"]
            c = triplet["c"]
            fixtures.append({
                "stem": stem,
                "eligibility": {"scope": "gated", "reasons": []},
                "alignment": {
                    "rust": {"selected_lag_samples": 1600},
                    "c": {"selected_lag_samples": 1600},
                },
                "encoded": {"rust": {"frame_count": 9},
                            "c": {"frame_count": 9}},
                "quality": {
                    "rust_vs_original": b1.measure_pair(
                        original, rust, lag_samples=1600,
                        reference_name="original", test_name="rust_decode"),
                    "c_vs_original": b1.measure_pair(
                        original, c, lag_samples=1600,
                        reference_name="original", test_name="c_decode"),
                    "rust_vs_c": b1.measure_pair(
                        rust, c, lag_samples=0,
                        reference_name="rust_decode", test_name="c_decode"),
                },
                "decoded": {
                    "rust": {"wav_sha256": hashlib.sha256(
                        (root / "decoded" / f"{stem}.rust.wav").read_bytes()
                    ).hexdigest()},
                    "c": {"wav_sha256": hashlib.sha256(
                        (root / "decoded" / f"{stem}.c.wav").read_bytes()
                    ).hexdigest()},
                },
            })
        path = root / "b1.json"
        path.write_text(json.dumps({
            "tool": {"name": "ferrite-aac-vbr5-metrics", "version": "test"},
            "manifest": {"sha256": "test"},
            "fixtures": fixtures,
        }), encoding="utf-8")
        return path

    def manifest(self, root: Path, stems: list[str],
                 triplets: dict[str, dict]) -> Path:
        import hashlib
        path = root / "manifest.json"
        base = json.loads((HERE / "fixtures-v3.json").read_text(encoding="utf-8"))
        fixtures = []
        for stem in stems:
            original = triplets[stem]["original"]
            fixtures.append({
                "stem": stem,
                "corpus_member": True,
                "category": "synthetic",
                "provenance_status": "deterministic-resynthesis",
                "provenance_source": "test",
                "license": "generated-test-fixture",
                "sample_rate_hz": 48000,
                "channels": 1,
                "samples_per_channel": original["samples_per_channel"],
                "duration_seconds": 0.125,
                "input_sha256": hashlib.sha256(
                    (root / "inputs" / f"{stem}.wav").read_bytes()).hexdigest(),
                "input_source": "generated:test",
                "alignment_gate": "48k",
                "eligibility": {"gated_aggregate": True, "reason": "test"},
            })
        base["fixtures"] = fixtures
        base["corpus_member_count"] = len(fixtures)
        path.write_text(json.dumps(base), encoding="utf-8")
        return path

    def test_deterministic_output(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = impulse()
            triplets = {"s": self.write_case(root, "s", original)}
            b1_json = self.b1_json(root, ["s"], triplets)
            manifest = self.manifest(root, ["s"], triplets)
            outs = []
            for name in ("a", "b"):
                out = root / name
                rc = loc.main([
                    "--manifest", str(manifest),
                    "--b1-json", str(b1_json),
                    "--b1-work-dir", str(root),
                    "--out-dir", str(out),
                ])
                self.assertEqual(rc, 0)
                outs.append((out / "localization.json").read_bytes())
            self.assertEqual(outs[0], outs[1])
            data = json.loads(outs[0])
            self.assertTrue(
                data["fixtures"][0]["per_channel"][0]["reconciliation"]["reconciled"])

    def test_excluded_fixture_is_skipped(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = impulse()
            triplets = {"s": self.write_case(root, "s", original)}
            b1_json = self.b1_json(root, ["s"], triplets)
            data = json.loads(b1_json.read_text(encoding="utf-8"))
            data["fixtures"][0]["quality"] = None
            data["fixtures"][0]["eligibility"] = {
                "scope": "excluded", "reasons": ["no_signal"]}
            b1_json.write_text(json.dumps(data), encoding="utf-8")
            manifest = self.manifest(root, ["s"], triplets)
            out = root / "out"
            rc = loc.main([
                "--manifest", str(manifest),
                "--b1-json", str(b1_json),
                "--b1-work-dir", str(root),
                "--out-dir", str(out),
            ])
            self.assertEqual(rc, 0)
            result = json.loads((out / "localization.json").read_text(encoding="utf-8"))
            self.assertEqual(result["fixtures"], [])
            self.assertEqual(len(result["skipped"]), 1)

    def test_lag_mismatch_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = impulse()
            triplets = {"s": self.write_case(root, "s", original)}
            b1_json = self.b1_json(root, ["s"], triplets)
            data = json.loads(b1_json.read_text(encoding="utf-8"))
            data["fixtures"][0]["alignment"]["rust"]["selected_lag_samples"] = 1601
            b1_json.write_text(json.dumps(data), encoding="utf-8")
            manifest = self.manifest(root, ["s"], triplets)
            rc = loc.main([
                "--manifest", str(manifest),
                "--b1-json", str(b1_json),
                "--b1-work-dir", str(root),
                "--out-dir", str(root / "out"),
            ])
            self.assertEqual(rc, 2)

    def test_hash_mismatch_is_refused(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            original = impulse()
            triplets = {"s": self.write_case(root, "s", original)}
            b1_json = self.b1_json(root, ["s"], triplets)
            # Corrupt the decoded file after its B1 hash was recorded.
            (root / "decoded" / "s.rust.wav").write_bytes(b"corrupted")
            manifest = self.manifest(root, ["s"], triplets)
            rc = loc.main([
                "--manifest", str(manifest),
                "--b1-json", str(b1_json),
                "--b1-work-dir", str(root),
                "--out-dir", str(root / "out"),
            ])
            self.assertEqual(rc, 2)


if __name__ == "__main__":
    unittest.main()
