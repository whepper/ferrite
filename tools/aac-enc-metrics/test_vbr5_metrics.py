#!/usr/bin/env python3
"""Targeted tests for tools/aac-enc-metrics/vbr5_metrics.py.

These tests do not encode or decode real AAC. They exercise the alignment
policy, single-lag delay accounting, decoded-length plausibility, parsers,
provenance gating, baseline reproduction semantics, and manifest eligibility
with small synthetic files.
"""

from __future__ import annotations

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
import vbr5_metrics as metrics  # noqa: E402


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


def impulse_signal(length: int = 6000, index: int = 2000) -> list[int]:
    signal = [0] * length
    signal[index] = 30000
    return signal


def delayed(signal: list[int], lag: int) -> list[int]:
    return [0] * lag + signal


def sine_signal(
    length: int = 60000,
    frequency: float = 100.0,
    sample_rate: int = 48000,
    amplitude: int = 20000,
) -> list[int]:
    return [
        int(round(amplitude * math.sin(2.0 * math.pi * frequency * i / sample_rate)))
        for i in range(length)
    ]


def contract() -> dict[str, object]:
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


def adts_frame(payload: bytes, *, crc: bool) -> bytes:
    header_length = 9 if crc else 7
    frame_length = header_length + len(payload)
    header = bytearray(header_length)
    header[0] = 0xFF
    header[1] = 0xF0 if crc else 0xF1
    header[2] = 0x40
    header[3] = (frame_length >> 11) & 0x03
    header[4] = (frame_length >> 3) & 0xFF
    header[5] = ((frame_length & 0x07) << 5) | 0x1F
    header[6] = 0xFC
    return bytes(header) + payload


def write_adts(path: Path, frames: list[tuple[bytes, bool]]) -> None:
    path.write_bytes(b"".join(adts_frame(payload, crc=crc) for payload, crc in frames))


def fake_rust_meta(**overrides: object) -> dict[str, object]:
    data: dict[str, object] = {
        "source_commit": "3878c45a8b9d85956bf1b62b17897f94c41b2d6a",
        "baseline_source_commit": "3878c45a8b9d85956bf1b62b17897f94c41b2d6a",
        "source_matches_baseline": True,
        "sources_dirty": False,
        "toolchain_matches_declared": True,
    }
    data.update(overrides)
    return data


def fake_rust_collection(**overrides: object) -> dict[str, object]:
    data: dict[str, object] = {"harness_ran": True, "adts_source": "cargo harness"}
    data.update(overrides)
    return data


def fake_c_meta(**overrides: object) -> dict[str, object]:
    data: dict[str, object] = {
        "encoder_exact": True,
        "decoder_exact": True,
        "source_matches_baseline": True,
        "source_clean": True,
        "library_matches_expected": True,
    }
    data.update(overrides)
    return data


def fake_c_collection(**overrides: object) -> dict[str, object]:
    data: dict[str, object] = {"encoder_ran": True, "adts_source": "C encoder"}
    data.update(overrides)
    return data


class AlignmentTests(unittest.TestCase):
    def test_unique_per_channel_stereo(self) -> None:
        original = impulse_signal()
        decoded = delayed(original, 1600)
        original_pcm = {
            "channels": 2,
            "samples_per_channel": len(original),
            "pcm": [original, original],
        }
        decoded_pcm = {
            "channels": 2,
            "samples_per_channel": len(decoded),
            "pcm": [decoded, decoded],
        }
        result = metrics.estimate_alignment(original_pcm, decoded_pcm, contract())
        self.assertEqual(result["ambiguity_status"], "unique")
        self.assertEqual(result["selected_lag_samples"], 1600)
        self.assertEqual(result["channel_lag_delta_samples"], 0)
        self.assertFalse(result["ambiguous"])
        self.assertEqual(result["coarse_step_samples"], 1)
        self.assertEqual(result["per_channel"][0]["refine_half_width_samples"], 2)

    def test_unique_mono(self) -> None:
        original = impulse_signal()
        decoded = delayed(original, 1600)
        result = metrics.estimate_alignment(
            {"channels": 1, "samples_per_channel": len(original), "pcm": [original]},
            {"channels": 1, "samples_per_channel": len(decoded), "pcm": [decoded]},
            contract(),
        )
        self.assertEqual(result["ambiguity_status"], "unique")
        self.assertEqual(result["selected_lag_samples"], 1600)
        self.assertEqual(result["channel_lag_delta_samples"], None)

    def test_channel_disagreement_is_excluded(self) -> None:
        original = impulse_signal()
        original_pcm = {
            "channels": 2,
            "samples_per_channel": len(original),
            "pcm": [original, original],
        }
        decoded_pcm = {
            "channels": 2,
            "samples_per_channel": len(original) + 1700,
            "pcm": [delayed(original, 1600), delayed(original, 1606)],
        }
        result = metrics.estimate_alignment(original_pcm, decoded_pcm, contract())
        self.assertEqual(result["ambiguity_status"], "channel_lag_disagreement")
        self.assertIsNone(result["selected_lag_samples"])
        self.assertTrue(result["ambiguous"])
        self.assertEqual(result["channel_lag_delta_samples"], 6)

    def test_no_signal_is_ambiguous(self) -> None:
        silent = [0] * 6000
        pcm = {"channels": 1, "samples_per_channel": len(silent), "pcm": [silent]}
        result = metrics.estimate_alignment(pcm, pcm, contract())
        self.assertEqual(result["ambiguity_status"], "no_signal")
        self.assertIsNone(result["selected_lag_samples"])
        self.assertTrue(result["ambiguous"])

    def test_lower_search_boundary_is_ambiguous(self) -> None:
        original = impulse_signal()
        decoded = delayed(original, 1568)  # exactly the lower search bound
        original_pcm = {
            "channels": 1,
            "samples_per_channel": len(original),
            "pcm": [original],
        }
        decoded_pcm = {
            "channels": 1,
            "samples_per_channel": len(decoded),
            "pcm": [decoded],
        }
        result = metrics.estimate_alignment(original_pcm, decoded_pcm, contract())
        self.assertEqual(result["ambiguity_status"], "search_boundary")
        self.assertIsNone(result["selected_lag_samples"])
        self.assertTrue(result["ambiguous"])

    def test_upper_search_boundary_is_ambiguous(self) -> None:
        original = impulse_signal()
        decoded = delayed(original, 1632)  # exactly the upper search bound
        result = metrics.estimate_alignment(
            {"channels": 1, "samples_per_channel": len(original), "pcm": [original]},
            {"channels": 1, "samples_per_channel": len(decoded), "pcm": [decoded]},
            contract(),
        )
        self.assertEqual(result["ambiguity_status"], "search_boundary")
        self.assertIsNone(result["selected_lag_samples"])

    def test_score_tie_is_excluded(self) -> None:
        result = metrics.classify_lag_candidates(
            [(1600, 1.0), (1601, 0.5), (1602, 1.0)],
            [(1600, 1.0), (1602, 1.0)],
            lower=1568,
            upper=1632,
            tie_epsilon=1e-9,
        )
        self.assertEqual(result["status"], "score_tie")
        self.assertEqual(result["score_margin"], 0.0)
        self.assertEqual(result["estimated_lag_samples"], 1600)

    def test_nominal_lag_outside_tolerance_is_excluded(self) -> None:
        original = impulse_signal()
        # 1603 is inside the search range but outside nominal +/-1.
        decoded = delayed(original, 1603)
        result = metrics.estimate_alignment(
            {"channels": 1, "samples_per_channel": len(original), "pcm": [original]},
            {"channels": 1, "samples_per_channel": len(decoded), "pcm": [decoded]},
            contract(),
        )
        self.assertEqual(result["ambiguity_status"], "outside_nominal_tolerance")
        self.assertIsNone(result["selected_lag_samples"])

    def test_long_signal_stride_and_refinement(self) -> None:
        original = sine_signal(length=60000)
        # 1601 is inside nominal +/-1 and inside the stride-8 coarse cell for
        # 1600. The implementation must refine +/-4 and select it.
        decoded = delayed(original, 1601)
        result = metrics.estimate_alignment(
            {"channels": 1, "samples_per_channel": len(original), "pcm": [original]},
            {"channels": 1, "samples_per_channel": len(decoded), "pcm": [decoded]},
            contract(),
        )
        self.assertEqual(result["coarse_step_samples"], 8)
        self.assertEqual(result["per_channel"][0]["refine_half_width_samples"], 4)
        self.assertEqual(result["per_channel"][0]["estimated_lag_samples"], 1601)
        # The nominal prior wins when both channel estimates are within +/-1,
        # but the refinement itself must still have found 1601.
        self.assertEqual(result["selected_lag_samples"], 1600)
        self.assertEqual(
            result["selection_method"], "nominal_prior_within_tolerance"
        )
        self.assertEqual(result["ambiguity_status"], "unique")


class MetricTests(unittest.TestCase):
    def test_delay_is_subtracted_once(self) -> None:
        original = impulse_signal()
        # One outputDelay-like pre-roll is already represented by the 1600
        # samples of decoder priming. The metric must apply only the selected
        # lag, never also a decoder outputDelay.
        decoded_signal = delayed(original, 1600)
        # Add a deterministic small error so SNR is defined rather than
        # "identical"; this does not change the alignment.
        decoded_signal = [
            value + (1 if i % 7 == 0 else 0)
            for i, value in enumerate(decoded_signal)
        ]
        original_pcm = {
            "channels": 1,
            "samples_per_channel": len(original),
            "pcm": [original],
        }
        decoded_pcm = {
            "channels": 1,
            "samples_per_channel": len(decoded_signal),
            "pcm": [decoded_signal],
        }
        result = metrics.measure_pair(
            original_pcm,
            decoded_pcm,
            lag_samples=1600,
            reference_name="original",
            test_name="decoded",
        )
        self.assertEqual(result["lag_samples"], 1600)
        self.assertEqual(result["overlap_samples_per_channel"], len(original))
        self.assertEqual(result["overlap_samples_total"], len(original))
        self.assertEqual(result["aggregate"]["samples_compared"], len(original))
        self.assertGreaterEqual(result["aggregate"]["peak_error_lsb"], 0)
        self.assertEqual(
            result["aggregate"]["sdr_db"], result["aggregate"]["snr_db"]
        )
        self.assertTrue(result["aggregate"]["sdr_db_is_alias"])

    def test_identical_signal_status(self) -> None:
        signal = impulse_signal()
        pcm = {"channels": 1, "samples_per_channel": len(signal), "pcm": [signal]}
        result = metrics.measure_pair(
            pcm, pcm, lag_samples=0, reference_name="a", test_name="b"
        )
        self.assertEqual(result["aggregate"]["snr_status"], "identical")
        self.assertIsNone(result["aggregate"]["snr_db"])
        self.assertEqual(result["aggregate"]["rms_error_lsb"], 0.0)

    def test_tail_shortfall_is_explicit(self) -> None:
        original = impulse_signal(length=6000)
        decoded = original  # same length, but a 1600 lag trims the reference tail
        result = metrics.measure_pair(
            {"channels": 1, "samples_per_channel": len(original), "pcm": [original]},
            {"channels": 1, "samples_per_channel": len(decoded), "pcm": [decoded]},
            lag_samples=1600,
            reference_name="original",
            test_name="decoded",
        )
        self.assertEqual(result["overlap_samples_per_channel"], 4400)
        self.assertEqual(
            result["reference_tail_shortfall_samples_per_channel"], 1600
        )
        self.assertEqual(result["test_tail_shortfall_samples_per_channel"], 0)

    def test_decoded_length_plausibility(self) -> None:
        plausible = metrics.decoded_length_check(
            frame_count=49,
            decoded_samples_per_channel=48432,
            sample_rate_hz=48000,
            output_delays={"48000": 1744, "44100": 1685},
            frame_length_samples=1024,
        )
        self.assertEqual(plausible["status"], "plausible")
        self.assertTrue(plausible["plausible"])
        self.assertEqual(plausible["expected_samples_per_channel"], 48432)

        short = metrics.decoded_length_check(
            frame_count=49,
            decoded_samples_per_channel=48431,
            sample_rate_hz=48000,
            output_delays={"48000": 1744, "44100": 1685},
            frame_length_samples=1024,
        )
        self.assertEqual(short["status"], "implausible")
        self.assertFalse(short["plausible"])
        self.assertEqual(short["delta_samples"], -1)

        forty_one = metrics.decoded_length_check(
            frame_count=45,
            decoded_samples_per_channel=44395,
            sample_rate_hz=44100,
            output_delays={"48000": 1744, "44100": 1685},
            frame_length_samples=1024,
        )
        self.assertTrue(forty_one["plausible"])

        unavailable = metrics.decoded_length_check(
            frame_count=10,
            decoded_samples_per_channel=1000,
            sample_rate_hz=32000,
            output_delays={"48000": 1744, "44100": 1685},
            frame_length_samples=1024,
        )
        self.assertEqual(unavailable["status"], "unavailable")
        self.assertIsNone(unavailable["plausible"])

    def test_structurally_valid_but_short_wav_is_implausible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "short.wav"
            write_wav(path, [[100] * 100])
            wav = metrics.read_wav_i16(path)
            check = metrics.decoded_length_check(
                frame_count=49,
                decoded_samples_per_channel=wav["samples_per_channel"],
                sample_rate_hz=wav["sample_rate_hz"],
                output_delays={"48000": 1744},
                frame_length_samples=1024,
            )
        self.assertEqual(check["status"], "implausible")
        self.assertLess(check["delta_samples"], 0)

    def test_clipping_count(self) -> None:
        pcm = [[0, 32767, -32768, -32767], [1, 2, 3, 4]]
        self.assertEqual(metrics.count_clipped_samples(pcm), 3)

    def test_wav_parser_roundtrip(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "stereo.wav"
            left = [0, 100, -100, 32767]
            right = [1, -1, 2, -2]
            write_wav(path, [left, right])
            wav = metrics.read_wav_i16(path)
        self.assertEqual(wav["channels"], 2)
        self.assertEqual(wav["sample_rate_hz"], 48000)
        self.assertEqual(wav["samples_per_channel"], 4)
        self.assertEqual(wav["pcm"][0], left)
        self.assertEqual(wav["pcm"][1], right)

    def test_wav_parser_rejects_non_16_bit(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "eight.wav"
            with wave.open(str(path), "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(1)
                w.setframerate(48000)
                w.writeframes(b"\x80" * 100)
            with self.assertRaises(metrics.MetricsError):
                metrics.read_wav_i16(path)

    def test_wav_parser_rejects_truncated_pcm(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "truncated.wav"
            write_wav(path, [[1, 2, 3, 4]])
            data = path.read_bytes()
            path.write_bytes(data[:-2])
            with self.assertRaises(metrics.MetricsError):
                metrics.read_wav_i16(path)

    def test_adts_parser(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "two.aac"
            write_adts(path, [(b"a" * 10, False), (b"b" * 20, True)])
            parsed = metrics.parse_adts(path)
        self.assertEqual(parsed["frame_count"], 2)
        self.assertEqual(parsed["total_bytes"], 46)
        self.assertEqual(parsed["header_bytes"], 16)
        self.assertEqual(parsed["payload_bytes"], 30)
        self.assertEqual(parsed["frame_length_bytes"]["min_bytes"], 17)
        self.assertEqual(parsed["frame_length_bytes"]["max_bytes"], 29)

    def test_adts_parser_rejects_trailing_bytes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "trailing.aac"
            write_adts(path, [(b"a" * 10, False)])
            path.write_bytes(path.read_bytes() + b"\x00")
            with self.assertRaises(metrics.MetricsError):
                metrics.parse_adts(path)

    def test_adts_parser_rejects_sync_loss(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sync.aac"
            write_adts(path, [(b"a" * 10, False)])
            data = bytearray(path.read_bytes())
            data[0] = 0x00
            path.write_bytes(bytes(data))
            with self.assertRaises(metrics.MetricsError):
                metrics.parse_adts(path)

    def test_adts_parser_rejects_empty(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "empty.aac"
            path.write_bytes(b"")
            with self.assertRaises(metrics.MetricsError):
                metrics.parse_adts(path)


class ProvenanceTests(unittest.TestCase):
    def provenance(
        self,
        *,
        rust_meta: dict[str, object] | None = None,
        rust_collection: dict[str, object] | None = None,
        c_meta: dict[str, object] | None = None,
        c_collection: dict[str, object] | None = None,
        v3_item: dict[str, object] | None = None,
        override: bool = False,
    ) -> dict[str, object]:
        return metrics.artifact_provenance(
            "fixture",
            rust_meta=rust_meta or fake_rust_meta(),
            rust_collection=rust_collection or fake_rust_collection(),
            c_meta=c_meta or fake_c_meta(),
            c_collection=c_collection or fake_c_collection(),
            v3_item=v3_item,
            allow_different_reference=override,
        )

    def test_exact_encoder_not_run_with_pregenerated_adts_is_unverified(self) -> None:
        result = self.provenance(
            c_collection=fake_c_collection(encoder_ran=False),
            v3_item=None,
        )
        self.assertFalse(result["c"]["verified"])
        self.assertFalse(result["both_verified"])
        self.assertIn("pre-generated C ADTS", result["c"]["reason"])
        self.assertIn("encoder binary hash alone", result["c"]["reason"])

    def test_pregenerated_rust_without_retained_match_is_unverified(self) -> None:
        result = self.provenance(
            rust_collection=fake_rust_collection(harness_ran=False),
            v3_item=None,
        )
        self.assertFalse(result["rust"]["verified"])
        self.assertFalse(result["both_verified"])
        self.assertIn("pre-generated Rust ADTS", result["rust"]["reason"])

    def test_retained_v3_match_verifies_both_streams(self) -> None:
        result = self.provenance(
            rust_collection=fake_rust_collection(harness_ran=False),
            c_collection=fake_c_collection(encoder_ran=False),
            v3_item={"rust_adts": True, "c_adts": True},
        )
        self.assertTrue(result["rust"]["verified"])
        self.assertTrue(result["c"]["verified"])
        self.assertTrue(result["both_verified"])
        self.assertTrue(result["rust"]["retained_v3_adts_match"])

    def test_current_run_verified_with_exact_reference(self) -> None:
        result = self.provenance(v3_item=None)
        self.assertTrue(result["rust"]["verified"])
        self.assertTrue(result["c"]["verified"])
        self.assertTrue(result["both_verified"])
        self.assertTrue(result["rust"]["current_run_source_verified"])
        self.assertTrue(result["c"]["current_run_encoder_verified"])

    def test_partial_retained_match_leaves_unmatched_stream_unverified(self) -> None:
        result = self.provenance(
            rust_collection=fake_rust_collection(harness_ran=False),
            c_collection=fake_c_collection(encoder_ran=False),
            v3_item={"rust_adts": True, "c_adts": False},
        )
        self.assertTrue(result["rust"]["verified"])
        self.assertFalse(result["c"]["verified"])
        self.assertFalse(result["both_verified"])

    def test_override_forces_unverified_even_with_v3_match(self) -> None:
        result = self.provenance(
            v3_item={"rust_adts": True, "c_adts": True},
            override=True,
        )
        self.assertFalse(result["rust"]["verified"])
        self.assertFalse(result["c"]["verified"])
        self.assertFalse(result["both_verified"])
        self.assertIn("override", result["rust"]["reason"])

    def test_later_rust_revision_without_match_is_unverified(self) -> None:
        result = self.provenance(
            rust_meta=fake_rust_meta(
                source_commit="later",
                source_matches_baseline=False,
            ),
            v3_item=None,
        )
        self.assertFalse(result["rust"]["verified"])
        self.assertFalse(result["both_verified"])
        self.assertIn("is not the baseline", result["rust"]["reason"])

    def test_later_rust_revision_with_match_is_verified(self) -> None:
        result = self.provenance(
            rust_meta=fake_rust_meta(
                source_commit="later",
                source_matches_baseline=False,
            ),
            v3_item={"rust_adts": True, "c_adts": True},
        )
        self.assertTrue(result["rust"]["verified"])
        self.assertTrue(result["both_verified"])

    def test_dirty_rust_tree_blocks_current_run(self) -> None:
        result = self.provenance(
            rust_meta=fake_rust_meta(sources_dirty=True),
            v3_item=None,
        )
        self.assertFalse(result["rust"]["verified"])
        self.assertIn("dirty", result["rust"]["reason"])

    def test_dirty_c_source_blocks_current_run(self) -> None:
        result = self.provenance(
            c_meta=fake_c_meta(source_clean=False),
            v3_item=None,
        )
        self.assertFalse(result["c"]["verified"])
        self.assertIn("dirty", result["c"]["reason"])

    def test_scope_decisions(self) -> None:
        base = {
            "static_gate": True,
            "alignment_ok": True,
            "stream_provenance_ok": True,
            "decoded_length_state_value": "verified",
        }
        self.assertEqual(metrics.decide_scope(**base), "gated")
        self.assertEqual(
            metrics.decide_scope(**{**base, "stream_provenance_ok": False}),
            "exploratory",
        )
        self.assertEqual(
            metrics.decide_scope(**{**base, "static_gate": False}),
            "exploratory",
        )
        self.assertEqual(
            metrics.decide_scope(**{**base, "alignment_ok": False}),
            "excluded",
        )
        self.assertEqual(
            metrics.decide_scope(
                **{**base, "decoded_length_state_value": "implausible"}
            ),
            "excluded",
        )
        self.assertEqual(
            metrics.decide_scope(
                **{**base, "decoded_length_state_value": "unavailable"}
            ),
            "exploratory",
        )


class BaselineTests(unittest.TestCase):
    CLASSES = (
        "input",
        "rust_adts",
        "c_adts",
        "rust_decoded_wav",
        "c_decoded_wav",
    )

    def comparison(
        self, *, count: int, matched: int | None = None
    ) -> dict[str, object]:
        matched = count if matched is None else matched
        return {
            "summary": {
                key: {
                    "matched": matched,
                    "compared": count,
                    "expected": count,
                }
                for key in self.CLASSES
            }
        }

    def fixtures(self, count: int) -> list[dict[str, object]]:
        return [{"stem": f"fixture-{i}"} for i in range(count)]

    def test_full_corpus_reproduction_requires_19_of_19_in_every_class(self) -> None:
        status = metrics.baseline_reproduction_status(
            self.fixtures(19), self.comparison(count=19)
        )
        self.assertTrue(status["baseline_reproduced"])
        self.assertTrue(status["baseline_reproduced_for_selection"])
        self.assertEqual(status["scope"], "full-corpus")

    def test_one_fixture_subset_is_not_full_baseline_reproduction(self) -> None:
        status = metrics.baseline_reproduction_status(
            self.fixtures(1), self.comparison(count=1)
        )
        self.assertIsNone(status["baseline_reproduced"])
        self.assertTrue(status["baseline_reproduced_for_selection"])
        self.assertEqual(status["scope"], "selection")
        self.assertIn("does not establish full-corpus", status["note"])

    def test_full_corpus_with_one_mismatch_is_false(self) -> None:
        comparison = self.comparison(count=19)
        comparison["summary"]["c_adts"]["matched"] = 18  # type: ignore[index]
        status = metrics.baseline_reproduction_status(
            self.fixtures(19), comparison
        )
        self.assertFalse(status["baseline_reproduced"])
        self.assertFalse(status["baseline_reproduced_for_selection"])

    def test_missing_comparison_is_not_checked(self) -> None:
        status = metrics.baseline_reproduction_status(self.fixtures(19), None)
        self.assertIsNone(status["baseline_reproduced"])

    def test_compare_v3_missing_artifact_is_not_matched(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            actual = root / "input.wav"
            actual.write_bytes(b"actual")
            v3 = root / "v3"
            (v3 / "inputs").mkdir(parents=True)
            # Deliberately omit the retained input.
            result = metrics.compare_v3(
                [{"stem": "fixture"}],
                v3_dir=v3,
                inputs={"fixture": actual},
                rust_adts={"fixture": actual},
                c_adts={"fixture": actual},
                decoded={"fixture": {"rust": actual, "c": actual}},
            )
        self.assertIsNone(result["per_fixture"]["fixture"]["input"])
        self.assertFalse(result["selection_complete"])
        self.assertFalse(result["all_matched"])

    def test_compare_v3_uses_stable_label_across_roots(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            actual = {
                "input": root / "input.wav",
                "rust_adts": root / "rust.aac",
                "c_adts": root / "c.aac",
                "rust_wav": root / "rust.wav",
                "c_wav": root / "c.wav",
            }
            for path in actual.values():
                path.write_bytes(path.name.encode() + b" contents")

            outputs = []
            for name in ("root-a", "root-b"):
                v3 = root / name
                (v3 / "inputs").mkdir(parents=True)
                (v3 / "rust-adts").mkdir()
                (v3 / "c-adts").mkdir()
                (v3 / "rust-wav").mkdir()
                (v3 / "c-wav").mkdir()
                (v3 / "inputs" / "fixture.wav").write_bytes(
                    actual["input"].read_bytes()
                )
                (v3 / "rust-adts" / "fixture.rust.aac").write_bytes(
                    actual["rust_adts"].read_bytes()
                )
                (v3 / "c-adts" / "fixture.c.aac").write_bytes(
                    actual["c_adts"].read_bytes()
                )
                (v3 / "rust-wav" / "fixture.rust.wav").write_bytes(
                    actual["rust_wav"].read_bytes()
                )
                (v3 / "c-wav" / "fixture.c.wav").write_bytes(
                    actual["c_wav"].read_bytes()
                )
                outputs.append(
                    metrics.compare_v3(
                        [{"stem": "fixture"}],
                        v3_dir=v3,
                        inputs={"fixture": actual["input"]},
                        rust_adts={"fixture": actual["rust_adts"]},
                        c_adts={"fixture": actual["c_adts"]},
                        decoded={
                            "fixture": {
                                "rust": actual["rust_wav"],
                                "c": actual["c_wav"],
                            }
                        },
                    )
                )
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(outputs[0]["reference_label"], "retained-v3")
        self.assertTrue(outputs[0]["all_matched"])


class ReferenceTests(unittest.TestCase):
    def test_toolchain_declaration_comparison(self) -> None:
        self.assertTrue(
            metrics.toolchain_matches_declared(
                "rustc/cargo 1.99.1",
                "rustc 1.99.1 (58cf8f9c3 2026-10-13)",
                "cargo 1.99.1 (5f94df478 2026-08-27)",
            )
        )
        self.assertFalse(
            metrics.toolchain_matches_declared(
                "rustc/cargo 1.99.1",
                "rustc 1.98.0 (aaaaaaaaa 2026-01-01)",
                "cargo 1.98.0 (bbbbbbbbb 2026-01-01)",
            )
        )
        self.assertIsNone(
            metrics.toolchain_matches_declared("unknown", "rustc 1.99.1", "cargo 1.99.1")
        )

    def test_c_source_cleanliness_is_measured(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            metrics.run_command(["git", "init", "-q", str(repo)])
            metrics.run_command(["git", "-C", str(repo), "config", "user.email", "test@example.com"])
            metrics.run_command(["git", "-C", str(repo), "config", "user.name", "Test"])
            tracked = repo / "tracked.txt"
            tracked.write_text("clean\n", encoding="utf-8")
            metrics.run_command(["git", "-C", str(repo), "add", "tracked.txt"])
            metrics.run_command(["git", "-C", str(repo), "commit", "-q", "-m", "initial"])
            commit = metrics.run_command(
                ["git", "-C", str(repo), "rev-parse", "HEAD"]
            ).stdout.strip()

            clean = metrics.c_source_metadata(
                repo, commit, allow_different_reference=False
            )
            self.assertTrue(clean["clean"])
            self.assertTrue(clean["matches_baseline"])

            tracked.write_text("dirty\n", encoding="utf-8")
            dirty = metrics.c_source_metadata(
                repo, commit, allow_different_reference=False
            )
            self.assertFalse(dirty["clean"])
            self.assertEqual(dirty["dirty_paths"], ["tracked.txt"])
            self.assertIn("dirty", dirty["status"])

    def test_c_source_mismatch_requires_override(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo = Path(tmp)
            metrics.run_command(["git", "init", "-q", str(repo)])
            metrics.run_command(["git", "-C", str(repo), "config", "user.email", "test@example.com"])
            metrics.run_command(["git", "-C", str(repo), "config", "user.name", "Test"])
            (repo / "tracked.txt").write_text("clean\n", encoding="utf-8")
            metrics.run_command(["git", "-C", str(repo), "add", "tracked.txt"])
            metrics.run_command(["git", "-C", str(repo), "commit", "-q", "-m", "initial"])
            with self.assertRaises(metrics.MetricsError):
                metrics.c_source_metadata(
                    repo, "f" * 40, allow_different_reference=False
                )
            result = metrics.c_source_metadata(
                repo, "f" * 40, allow_different_reference=True
            )
            self.assertFalse(result["matches_baseline"])
            self.assertIn("override", result["status"])


    def test_resolve_library_prefers_actual_rpath(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            bin_dir = root / "bin"
            rpath_dir = root / "ref-build"
            bin_dir.mkdir()
            rpath_dir.mkdir()
            decoder = bin_dir / "aac-dec"
            decoder.write_bytes(b"decoder")
            (bin_dir / "libfdk-aac.2.0.4.dylib").write_bytes(b"sibling")
            (rpath_dir / "libfdk-aac.2.dylib").write_bytes(b"rpath")
            path, lookup = metrics.resolve_c_library(
                None,
                None,
                decoder,
                {
                    "linked_libraries": ["@rpath/libfdk-aac.2.dylib"],
                    "rpaths": [str(rpath_dir)],
                },
            )
        self.assertEqual(path, rpath_dir / "libfdk-aac.2.dylib")
        self.assertEqual(lookup["method"], "rpath")

    def test_resolve_library_falls_back_to_sibling(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            decoder = root / "aac-dec"
            decoder.write_bytes(b"decoder")
            sibling = root / "libfdk-aac.2.0.4.dylib"
            sibling.write_bytes(b"sibling")
            path, lookup = metrics.resolve_c_library(
                None,
                None,
                decoder,
                {"linked_libraries": [], "rpaths": []},
            )
        self.assertEqual(path, sibling)
        self.assertEqual(lookup["method"], "sibling")

    def test_resolve_library_reports_unavailable(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            decoder = root / "aac-dec"
            decoder.write_bytes(b"decoder")
            path, lookup = metrics.resolve_c_library(
                None,
                None,
                decoder,
                {"linked_libraries": [], "rpaths": []},
            )
        self.assertIsNone(path)
        self.assertEqual(lookup["status"], "unavailable")


class ManifestTests(unittest.TestCase):
    def load(self) -> dict[str, object]:
        return json.loads((HERE / "fixtures-v3.json").read_text(encoding="utf-8"))

    def test_manifest_contract(self) -> None:
        manifest = self.load()
        metrics.validate_manifest(manifest)
        fixtures = manifest["fixtures"]
        corpus = [f for f in fixtures if f["corpus_member"]]
        self.assertEqual(len(corpus), 19)
        self.assertEqual(len([f for f in corpus if f["category"] == "synthetic"]), 15)
        self.assertEqual(
            len(
                [
                    f
                    for f in corpus
                    if f["category"] in ("licensed-speech", "licensed-music")
                ]
            ),
            3,
        )
        self.assertTrue(any(f["stem"] == "m6-stress-stereo-48k" for f in corpus))
        m8 = next(f for f in corpus if f["stem"] == "m8-music-eroica-stereo-48k")
        self.assertEqual(m8["provenance_status"], "provisional")
        self.assertFalse(m8["eligibility"]["gated_aggregate"])
        for fixture in corpus:
            if fixture["sample_rate_hz"] == 44100:
                self.assertFalse(fixture["eligibility"]["gated_aggregate"])
        mono = next(f for f in fixtures if f["stem"] == "m3-onset-mono-441k")
        self.assertFalse(mono["corpus_member"])
        self.assertEqual(mono["sample_rate_hz"], 44100)
        self.assertEqual(mono["channels"], 1)
        self.assertFalse(mono["eligibility"]["gated_aggregate"])

    def test_manifest_declares_long_signal_refinement(self) -> None:
        manifest = self.load()
        alignment = manifest["alignment"]
        self.assertEqual(alignment["refine_half_width_samples"], 2)
        self.assertEqual(alignment["long_signal_refine_half_width_samples"], 4)
        self.assertGreaterEqual(
            alignment["long_signal_refine_half_width_samples"],
            math.ceil(alignment["coarse_step_samples"] / 2),
        )

    def test_manifest_rejects_insufficient_long_refinement(self) -> None:
        manifest = self.load()
        manifest["alignment"]["long_signal_refine_half_width_samples"] = 2
        with self.assertRaises(metrics.MetricsError):
            metrics.validate_manifest(manifest)

    def test_committed_wav_hashes(self) -> None:
        manifest = self.load()
        repo_root = HERE.parents[1]
        for fixture in manifest["fixtures"]:
            source = fixture["input_source"]
            if source.startswith("committed:"):
                path = repo_root / source.split(":", 1)[1]
                self.assertEqual(metrics.sha256_file(path), fixture["input_sha256"])

    def test_separate_probe_hash_validation(self) -> None:
        manifest = self.load()
        probe = next(
            f for f in manifest["fixtures"] if not f["corpus_member"]
        )
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "probe.wav"
            write_wav(path, [[1, 2, 3, 4]], sample_rate=44100)
            probe["input_sha256"] = metrics.sha256_file(path)
            probe["input_path_hint"] = str(path)

            matched = metrics.verify_separate_probes(manifest)
            self.assertEqual(matched[0]["status"], "matched")
            self.assertEqual(matched[0]["actual_sha256"], probe["input_sha256"])

            probe["input_sha256"] = "0" * 64
            with self.assertRaises(metrics.MetricsError):
                metrics.verify_separate_probes(manifest)

            probe["input_sha256"] = metrics.sha256_file(path)
            probe["input_path_hint"] = str(Path(tmp) / "missing.wav")
            unavailable = metrics.verify_separate_probes(manifest)
            self.assertEqual(unavailable[0]["status"], "unavailable")
            self.assertIsNone(unavailable[0]["actual_sha256"])


if __name__ == "__main__":
    unittest.main()
