"""Independent checks for the paired measurement statistics and provenance gate."""

import argparse
import contextlib
import copy
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from collections.abc import Callable
from pathlib import Path
from typing import Any
from unittest.mock import Mock, patch

from format_campaign import Campaign, Cell
from format_report import RegressionGate, aggregate
from hypothesis import given, settings
from hypothesis import strategies as st


class FormatReportTests(unittest.TestCase):
    def test_deep_criterion_samples_are_not_silently_lost(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder).resolve()
            binary = root / "bench"
            binary.write_bytes(b"test binary")
            (root / "input.hymeko").write_text("Test{}", encoding="utf-8")
            (root / "flat-100.manifest.json").write_text(
                json.dumps([{"format": "hymeko", "file": "input.hymeko", "bytes": 6}]),
                encoding="utf-8",
            )
            directory = root / "samples"
            extended = (
                Path("\\\\?\\" + str(directory)) if os.name == "nt" else directory
            )
            samples = extended / "criterion" / ("a" * 120) / ("b" * 120) / "new"
            try:
                samples.mkdir(parents=True)
                (samples / "sample.json").write_text(
                    json.dumps({"times": [1_000_000] * 10, "iters": [1] * 10}),
                    encoding="utf-8",
                )
                adapter = Mock(
                    return_value={
                        "directory": str(directory),
                        "timeout": False,
                        "resources": {
                            "cap_ok": True,
                            "peak_observed": True,
                            "exit_code": 0,
                        },
                    }
                )
                campaign = Campaign(
                    argparse.Namespace(
                        output=root / "output",
                        fixtures=root,
                        binary=binary,
                        affinity_mask=1,
                        timeout=180,
                    ),
                    invoker=adapter,
                )
                result = campaign.run_cell(
                    Cell("flat", 100, "hymeko", "parse_document", 1)
                )
                self.assertEqual(result["measurement"]["samples_ms"], [1.0] * 10)
                if os.name == "nt":
                    # UNC output must keep its namespace, without contacting a share.
                    adapter.return_value["directory"] = r"\\server\share\output"
                    with (
                        patch.object(
                            Path, "resolve", autospec=True, side_effect=lambda p: p
                        ),
                        patch.object(
                            Path,
                            "glob",
                            autospec=True,
                            return_value=[samples / "sample.json"],
                        ) as scan,
                    ):
                        campaign.run_cell(
                            Cell("flat", 100, "hymeko", "parse_document", 2)
                        )
                    self.assertEqual(
                        str(scan.call_args.args[0]),
                        r"\\?\UNC\server\share\output\criterion",
                    )
            finally:
                # Only this test's freshly created TemporaryDirectory child.
                assert directory.resolve().is_relative_to(root)
                if extended.exists():
                    shutil.rmtree(extended)

    def test_platform_adapter_forwards_and_preserves_failure_gate(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            binary = root / "bench"
            binary.write_bytes(b"test binary")
            (root / "input.hymeko").write_text("Test{}", encoding="utf-8")
            (root / "flat-100.manifest.json").write_text(
                json.dumps([{"format": "hymeko", "file": "input.hymeko"}]),
                encoding="utf-8",
            )
            for cap_ok, timed_out, exit_code in [
                (True, False, 0),
                (False, False, 0),
                (True, True, 0),
                (True, False, 1),
            ]:
                invoker = Mock(
                    return_value={
                        "timeout": timed_out,
                        "resources": {
                            "cap_ok": cap_ok,
                            "peak_observed": True,
                            "exit_code": exit_code,
                        },
                    }
                )
                campaign = Campaign(
                    argparse.Namespace(
                        output=root / "output",
                        fixtures=root,
                        binary=binary,
                        affinity_mask=4,
                        timeout=180,
                    ),
                    invoker=invoker,
                )
                cell = Cell("flat", 100, "hymeko", "parse_document", 0)
                result = campaign.run_cell(cell, smoke=True)
                self.assertEqual(
                    result["status"] == "ok",
                    cap_ok and not timed_out and exit_code == 0,
                )
                invoker.assert_called_once_with(
                    cell.key,
                    {
                        "FORMAT_MODE": "bench",
                        "FORMAT_PATTERN": "flat",
                        "FORMAT_NODES": "100",
                        "FORMAT_FORMAT": "hymeko",
                        "FORMAT_PHASE": "parse_document",
                    },
                    "--bench --noplot --test",
                )

    def test_campaign_selection_and_reverse_round_order(self) -> None:
        campaign = object.__new__(Campaign)
        campaign.args = argparse.Namespace(
            formats=["hymeko"],
            phases=["parse_document", "decode_records"],
            patterns=["flat"],
            sizes=[100],
            rounds=2,
            mode="run",
        )
        cases = [
            (
                "run",
                ["parse_document", "decode_records"],
                [
                    Cell("flat", 100, "hymeko", "parse_document", 1),
                    Cell("flat", 100, "hymeko", "decode_records", 1),
                    Cell("flat", 100, "hymeko", "decode_records", 2),
                    Cell("flat", 100, "hymeko", "parse_document", 2),
                ],
            ),
            (
                "smoke",
                ["serialize", "parse_document"],
                [
                    Cell("linked", 100, "hymeko", "parse_document", 0),
                    Cell("long_text", 100, "hymeko", "parse_document", 0),
                ],
            ),
        ]
        for mode, phases, expected in cases:
            campaign.args.mode = mode
            campaign.args.phases = phases
            with (
                patch.object(
                    campaign, "run_cell", return_value={"resources": {}, "status": "ok"}
                ) as run,
                patch.object(campaign, "summarize"),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                campaign.run()
            self.assertEqual([c.args[0] for c in run.call_args_list], expected)

    def setUp(self) -> None:
        self.rows: list[dict[str, Any]] = [
            {
                "cell": {
                    "pattern": "flat",
                    "nodes": 1000,
                    "format": "json",
                    "phase": "decode_records",
                    "round": round_id,
                },
                "source_sha256": "s",
                "binary_sha256": "b",
                "affinity_mask": 4,
                "status": "ok",
                "timeout": False,
                "monitor_exit": 0,
                "wall_seconds": 1.0,
                "measurement": {"samples_ms": list(range(1, 11)), "median_ms": 5.5},
                "fixture": {"bytes": 1048576, "links": 0, "default_decode_error": None},
                "resources": {
                    "peak_working_set_mib": 16 + round_id,
                    "exit_code": 0,
                    "cap_ok": True,
                    "peak_observed": True,
                },
            }
            for round_id in (1, 2)
        ]
        self.campaign: dict[str, Any] = {
            "host_id": "test-host",
            "platform": "test-os",
            "machine": "test-cpu",
            "affinity_mask": 4,
            "binary_sha256": "b",
            "boundaries": {"decode_records": "owned output, cleanup included"},
            "cells": copy.deepcopy(self.rows),
        }
        for row in self.campaign["cells"]:
            row["measurement"]["samples_ms"] = [10.0] * 10

    def test_regression_gate_rejects_latency_memory_and_drift(self) -> None:
        gate = RegressionGate()
        for metric in ("latency", "peak"):
            changed = copy.deepcopy(self.campaign)
            for row in changed["cells"]:
                if metric == "latency":
                    row["measurement"]["samples_ms"] = [
                        x * 1.2 for x in row["measurement"]["samples_ms"]
                    ]
                else:
                    row["resources"]["peak_working_set_mib"] *= 1.2
            self.assertEqual(
                gate.compare(self.campaign, changed)["status"], "regression"
            )
        changed = copy.deepcopy(self.campaign)
        changed["cells"][0]["measurement"]["samples_ms"] = [
            x * 2 for x in self.campaign["cells"][0]["measurement"]["samples_ms"]
        ]
        self.assertEqual(gate.compare(self.campaign, changed)["status"], "inconclusive")
        changed["cells"][0]["measurement"]["median_ms"] = 5.5
        self.assertEqual(
            gate.compare(self.campaign, changed)["status"],
            "inconclusive",
            "saved medians must not hide raw sample drift",
        )

    def test_gate_rejects_invalid_or_unmatched_evidence(self) -> None:
        mutations: list[Callable[[dict[str, Any]], None]] = [
            lambda d: d.pop("host_id"),
            lambda d: d.update(host_id="other"),
            lambda d: d.update(affinity_mask=True),
            lambda d: d.update(boundaries=["decode_records"]),
            lambda d: d.update(cells=[]),
            lambda d: d["cells"].pop(),
            lambda d: d["cells"].append(copy.deepcopy(d["cells"][0])),
            lambda d: d["cells"][0].update(source_sha256="other"),
            lambda d: d["cells"][0].update(binary_sha256="other"),
            lambda d: d["cells"][0].update(timeout=True),
            lambda d: d["cells"][0]["cell"].update(nodes=0),
            lambda d: d["cells"][0].update(wall_seconds=181),
            lambda d: d["cells"][0]["measurement"].update(samples_ms=[1e308] * 10),
            lambda d: d["cells"][0]["resources"].update(peak_working_set_mib=10**1000),
            lambda d: d["cells"][0]["resources"].update(peak_observed=False),
            lambda d: d["cells"][0]["resources"].update(peak_working_set_mib=16385),
            lambda d: d["cells"][0]["measurement"].update(
                samples_ms=[float("nan")] * 10
            ),
            lambda d: d["cells"][0]["measurement"].update(samples_ms=[1] * 9),
            lambda d: d["boundaries"].update(decode_records="different boundary"),
        ]
        for mutate in mutations:
            changed = copy.deepcopy(self.campaign)
            mutate(changed)
            with self.assertRaises(ValueError):
                RegressionGate().compare(self.campaign, changed)

    @settings(max_examples=60, derandomize=True, deadline=None)
    @given(
        scale=st.floats(min_value=0.01, max_value=1),
        slowdown=st.floats(min_value=1.11, max_value=4),
    )
    def test_gate_identity_scaling_and_monotone_slowdown(
        self, scale: float, slowdown: float
    ) -> None:
        baseline = copy.deepcopy(self.campaign)
        for row in baseline["cells"]:
            row["measurement"]["samples_ms"] = [
                scale * x for x in row["measurement"]["samples_ms"]
            ]
        gate = RegressionGate()
        self.assertEqual(gate.compare(baseline, baseline)["status"], "pass")
        for factor in (slowdown, slowdown + 1):
            changed = copy.deepcopy(baseline)
            for row in changed["cells"]:
                row["measurement"]["samples_ms"] = [
                    factor * x for x in row["measurement"]["samples_ms"]
                ]
            self.assertEqual(gate.compare(baseline, changed)["status"], "regression")

    def test_gate_limits_and_cli_exit_codes(self) -> None:
        for value in (-0.1, float("nan"), float("inf"), 1.1):
            with self.assertRaises(ValueError):
                RegressionGate(regression=value)
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            baseline = root / "baseline.json"
            baseline.write_text(json.dumps(self.campaign), encoding="utf-8")
            for expected, multiplier in [(0, 1), (1, 2), (3, 1)]:
                changed = copy.deepcopy(self.campaign)
                for row in changed["cells"]:
                    row["measurement"]["samples_ms"] = [
                        x * multiplier for x in row["measurement"]["samples_ms"]
                    ]
                if expected == 3:
                    changed["cells"][0]["resources"]["peak_working_set_mib"] *= 2
                (root / "results.json").write_text(
                    json.dumps(changed), encoding="utf-8"
                )
                command = [
                    sys.executable,
                    str(Path(__file__).with_name("format_report.py")),
                    str(root),
                    "--baseline",
                    str(baseline),
                ]
                result = subprocess.run(
                    command, capture_output=True, text=True, check=False
                )
                self.assertEqual(
                    result.returncode, expected, result.stdout + result.stderr
                )
                self.assertTrue((root / "regression.json").is_file())
            (root / "results.json").write_text("[]", encoding="utf-8")
            self.assertEqual(
                subprocess.run(command, capture_output=True, check=False).returncode, 2
            )
            command[2] = str(root / "missing-directory")
            self.assertEqual(
                subprocess.run(command, capture_output=True, check=False).returncode, 2
            )

    def test_gate_exact_boundary_and_within_round_dispersion(self) -> None:
        for value, expected in [(11.0, "pass"), (11.001, "regression")]:
            changed = copy.deepcopy(self.campaign)
            for row in changed["cells"]:
                row["measurement"]["samples_ms"] = [value] * 10
            self.assertEqual(
                RegressionGate().compare(self.campaign, changed)["status"], expected
            )
        changed = copy.deepcopy(self.campaign)
        for row in changed["cells"]:
            row["measurement"]["samples_ms"] = [10.0] * 5 + [30.0] * 5
        self.assertEqual(
            RegressionGate().compare(self.campaign, changed)["status"], "inconclusive"
        )

    def test_known_distribution_units_and_round_order(self) -> None:
        result = aggregate(list(reversed(self.rows)))
        self.assertEqual(result["median_ms"], 5.5)
        self.assertEqual((result["q1_ms"], result["q3_ms"]), (3, 8))
        self.assertEqual(result["worst_ms"], 10)
        self.assertEqual(result["peak_mib"], 18)
        self.assertAlmostEqual(result["mib_per_second"], 1000 / 5.5)
        self.assertAlmostEqual(result["records_per_second"], 1e6 / 5.5)
        self.assertEqual(result["round_ratio"], 1)

    def test_mixed_provenance_and_missing_round_are_rejected(self) -> None:
        for field in ("source_sha256", "binary_sha256", "affinity_mask"):
            changed = copy.deepcopy(self.rows)
            changed[0][field] = "other"
            with self.assertRaises(AssertionError):
                aggregate(changed)
        with self.assertRaises(AssertionError):
            aggregate(self.rows[:1])

    def test_invalid_numeric_samples_are_rejected(self) -> None:
        for value in (0, -1, float("nan"), float("inf")):
            changed = copy.deepcopy(self.rows)
            changed[0]["measurement"]["samples_ms"][0] = value
            with self.assertRaises(AssertionError):
                aggregate(changed)


if __name__ == "__main__":
    unittest.main()
