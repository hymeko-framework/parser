"""Validate a complete two-round campaign and export numerical/reader artifacts."""

from __future__ import annotations

import argparse
import csv
import datetime
import hashlib
import json
import math
import statistics
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any

from format_campaign import FORMATS, PATTERNS, PHASES

PATTERN_DESCRIPTIONS = {
    "flat": "Independent records with identifier, finite number and short ASCII text.",
    "linked": "Eight signed, weighted links per record; 50,000 records have 400,000 links. Targets are stored, not semantically resolved.",
    "unicode": "Hungarian, Japanese and Greek text with escaped newline and quotes in every record.",
    "long_text": "1,000 ASCII text characters per record; 50,000 records contain 50 MB of text payload.",
    "nested": "Record trees arranged as chains of at most 16 records; size counts all records, not roots.",
}
PHASE_DESCRIPTIONS = {
    "serialize": "Identical owned records to UTF-8 text. Fixture generation and final text destruction excluded. HyMeKo uses a narrow benchmark writer, not the general IR writer.",
    "parse_document": "Resident UTF-8 to native HyMeKo AST, roxmltree DOM, or generic JSON/YAML values. Representations and ownership differ; this is a frontend comparison.",
    "decode_records": "Resident UTF-8 to identical owned records. HyMeKo constructs records through the full grammar's output builder; competitors deserialize directly. Temporary intermediates are included; final record destruction is excluded. Historical AST-projection baselines are identified by their binary provenance.",
}
LIBRARIES = {
    "hymeko": {
        "label": "HyMeKo",
        "color": "#7c3aed",
        "version": "parser 0.1.0 / typed output builder; exact revision in binary/source provenance",
    },
    "json": {"label": "JSON", "color": "#0284c7", "version": "serde_json 1.0.151"},
    "xml": {
        "label": "XML",
        "color": "#d97706",
        "version": "roxmltree 0.21.1 (DOM); quick-xml 0.42.0 (records / writing)",
    },
    "yaml": {
        "label": "YAML",
        "color": "#059669",
        "version": "serde-saphyr 1.2.0 / granit-parser 1.2.0; 5M nodes / 10M events",
    },
}


def aggregate(group: list[dict[str, Any]]) -> dict[str, Any]:
    """Require matching source/binary and two complete rounds; retain all sample means."""
    assert {r["cell"]["round"] for r in group} == {1, 2} and len(group) == 2
    assert len({r["source_sha256"] for r in group}) == 1
    assert len({r["binary_sha256"] for r in group}) == 1
    assert len({r.get("affinity_mask") for r in group}) == 1
    group = sorted(group, key=lambda r: r["cell"]["round"])
    samples = [v for r in group for v in r["measurement"]["samples_ms"]]
    assert len(samples) == 20 and all(math.isfinite(v) and v > 0 for v in samples)
    median = statistics.median(samples)
    q1, _, q3 = statistics.quantiles(samples, n=4, method="inclusive")
    cell = group[0]["cell"]
    fixture = group[0]["fixture"]
    rounds = [statistics.median(r["measurement"]["samples_ms"]) for r in group]
    return {
        **{k: v for k, v in cell.items() if k != "round"},
        "id": "-".join(str(cell[k]) for k in ("pattern", "nodes", "format", "phase")),
        "bytes": fixture["bytes"],
        "links": fixture["links"],
        "median_ms": median,
        "q1_ms": q1,
        "q3_ms": q3,
        "worst_ms": max(samples),
        "samples_ms": samples,
        "round_medians_ms": rounds,
        "round_ratio": max(rounds) / min(rounds),
        "mib_per_second": fixture["bytes"] / 2**20 / (median / 1000),
        "records_per_second": cell["nodes"] / (median / 1000),
        "peak_mib": max(r["resources"]["peak_working_set_mib"] for r in group),
        "source_sha256": group[0]["source_sha256"],
        "default_decode_error": fixture["default_decode_error"],
    }


class RegressionGate:
    """Compare matched two-round campaigns; uncertainty never counts as a pass.

    Inputs must describe the same host, affinity, matrix, fixtures and boundaries.
    Binary hashes may differ between campaigns but must agree within each one.
    This checks recorded evidence, not host isolation or statistical significance.
    """

    def __init__(self, regression: float = 0.10, drift: float = 0.10):
        if not all(math.isfinite(x) and 0 <= x <= 1 for x in (regression, drift)):
            raise ValueError("limits must be finite fractions between zero and one")
        self.regression = regression
        self.drift = drift

    @staticmethod
    def positive(value: Any) -> bool:
        return (
            isinstance(value, (float, int))
            and not isinstance(value, bool)
            and 0 < value <= sys.float_info.max
        )

    @staticmethod
    def validate_context(raw: dict[str, Any]) -> None:
        if not isinstance(raw, dict):
            raise TypeError("campaigns must be JSON objects")
        if not all(
            isinstance(raw.get(k), str) and raw[k]
            for k in ("host_id", "platform", "machine")
        ):
            raise ValueError("nonempty host, platform and machine identities required")
        affinity = raw.get("affinity_mask")
        if type(affinity) is not int or not 0 < affinity < 2**63:
            raise ValueError("a positive explicit CPU affinity mask is required")
        boundaries = raw.get("boundaries")
        if (
            not isinstance(boundaries, dict)
            or not boundaries
            or not all(
                isinstance(k, str) and isinstance(v, str) and k and v
                for k, v in boundaries.items()
            )
        ):
            raise ValueError("named timing boundaries with descriptions required")

    def validate_cell(self, row: dict[str, Any], raw: dict[str, Any]) -> None:
        resources = row["resources"]
        samples = row["measurement"]["samples_ms"]
        valid_process = (
            row["status"] == "ok"
            and row["timeout"] is False
            and row["monitor_exit"] == 0
            and resources["exit_code"] == 0
            and resources["cap_ok"] is True
            and resources["peak_observed"] is True
        )
        valid_values = (
            len(samples) == 10
            and all(self.positive(x) for x in samples)
            and self.positive(resources["peak_working_set_mib"])
            and resources["peak_working_set_mib"] <= 16384
            and self.positive(row["wall_seconds"])
            and row["wall_seconds"] <= 180
            and max(samples) <= row["wall_seconds"] * 1000
        )
        if not valid_process or not valid_values:
            raise ValueError("unsuccessful process, invalid samples or resource budget")
        if (
            type(row["cell"]["nodes"]) is not int
            or not 0 < row["cell"]["nodes"] <= 50000
            or not self.positive(row["fixture"]["bytes"])
            or row["fixture"]["bytes"] > 16 * 1024**3
        ):
            raise ValueError("positive input size and record count required")
        if (
            not row["source_sha256"]
            or row["binary_sha256"] != raw["binary_sha256"]
            or row["affinity_mask"] != raw["affinity_mask"]
            or row["cell"]["phase"] not in raw["boundaries"]
        ):
            raise ValueError("cell provenance or timing boundary mismatch")

    def groups(
        self, raw: dict[str, Any]
    ) -> dict[tuple[Any, ...], list[dict[str, Any]]]:
        groups: dict[tuple[Any, ...], list[dict[str, Any]]] = defaultdict(list)
        if not raw["cells"] or not raw["binary_sha256"]:
            raise ValueError("empty campaign or binary identity")
        for row in raw["cells"]:
            self.validate_cell(row, raw)
            key = tuple(row["cell"][k] for k in ("pattern", "nodes", "format", "phase"))
            groups[key].append(row)
        for group in groups.values():
            if len(group) != 2 or {r["cell"]["round"] for r in group} != {1, 2}:
                raise ValueError("each case requires exactly rounds one and two")
            if (
                len({r["source_sha256"] for r in group}) != 1
                or group[0]["fixture"] != group[1]["fixture"]
                or len({r["resources"].get("memory_scope") for r in group}) != 1
            ):
                raise ValueError("fixture or memory monitor changed between rounds")
        return dict(groups)

    def compare(
        self, baseline: dict[str, Any], candidate: dict[str, Any]
    ) -> dict[str, Any]:
        self.validate_context(baseline)
        self.validate_context(candidate)
        for key in ("host_id", "platform", "machine", "affinity_mask", "boundaries"):
            if not baseline.get(key) or baseline[key] != candidate.get(key):
                raise ValueError(f"missing or mismatched {key}")
        before, after = self.groups(baseline), self.groups(candidate)
        if before.keys() != after.keys():
            raise ValueError("baseline and candidate matrices differ")
        rows = []
        for case_key in sorted(before):
            old, new = before[case_key], after[case_key]
            if (
                old[0]["source_sha256"] != new[0]["source_sha256"]
                or old[0]["fixture"] != new[0]["fixture"]
                or old[0]["resources"].get("memory_scope")
                != new[0]["resources"].get("memory_scope")
            ):
                raise ValueError("baseline and candidate fixtures or monitors differ")
            rows.append(self.assess(aggregate(old), aggregate(new), old, new))
        states = {r["status"] for r in rows}
        status = (
            "regression"
            if "regression" in states
            else "inconclusive"
            if "inconclusive" in states
            else "pass"
        )
        return {
            "status": status,
            "limits": {"regression": self.regression, "drift": self.drift},
            "rows": rows,
        }

    def assess(
        self,
        before: dict[str, Any],
        after: dict[str, Any],
        old: list[dict[str, Any]],
        new: list[dict[str, Any]],
    ) -> dict[str, Any]:
        ratios = {
            "latency": after["median_ms"] / before["median_ms"],
            "peak": after["peak_mib"] / before["peak_mib"],
        }
        peak_spread = []
        for group in (old, new):
            peaks = [r["resources"]["peak_working_set_mib"] for r in group]
            peak_spread.append(max(peaks) / min(peaks))
        spread = max(before["round_ratio"], after["round_ratio"], *peak_spread)
        relative_iqr = max(
            (row["q3_ms"] - row["q1_ms"]) / row["median_ms"] for row in (before, after)
        )
        status = (
            "inconclusive"
            if spread > 1 + self.drift or relative_iqr > self.drift
            else "regression"
            if max(ratios.values()) > 1 + self.regression
            else "pass"
        )
        return {
            "id": before["id"],
            "status": status,
            "ratios": ratios,
            "max_round_ratio": spread,
            "max_relative_iqr": relative_iqr,
            "before": before,
            "after": after,
        }

    def run(self, baseline: Path, candidate: Path) -> dict[str, Any]:
        try:
            return self.compare(
                json.loads(baseline.read_text(encoding="utf-8")),
                json.loads(candidate.read_text(encoding="utf-8")),
            )
        except (OSError, ValueError, KeyError, TypeError, AssertionError) as error:
            return {"status": "invalid", "error": str(error)}


def export(directory: Path) -> None:
    raw_path = directory / "results.json"
    raw = json.loads(raw_path.read_text(encoding="utf-8"))
    expected = {
        (p, n, f, s, r)
        for p in PATTERNS
        for n in (100, 1000, 10000, 50000)
        for f in FORMATS
        for s in PHASES
        for r in (1, 2)
    }
    actual = {
        tuple(row["cell"][k] for k in ("pattern", "nodes", "format", "phase", "round"))
        for row in raw["cells"]
    }
    assert actual == expected and len(raw["cells"]) == 480, (
        "Incomplete or duplicated matrix"
    )
    groups: dict[tuple[Any, ...], list[dict[str, Any]]] = defaultdict(list)
    for row in raw["cells"]:
        assert row["status"] == "ok" and row["binary_sha256"] == raw["binary_sha256"]
        assert row["resources"]["cap_ok"] and row["resources"]["peak_observed"]
        groups[
            tuple(row["cell"][k] for k in ("pattern", "nodes", "format", "phase"))
        ].append(row)
    rows = [aggregate(group) for group in groups.values()]
    summary = {
        "created_at": datetime.datetime.now().astimezone().isoformat(),
        "platform": raw["platform"],
        "cpu": "Intel Core Ultra 9 275HX; 24 cores; 63.4 GiB RAM",
        "binary_sha256": raw["binary_sha256"],
        "raw_sha256": hashlib.sha256(raw_path.read_bytes()).hexdigest(),
        "patterns": PATTERN_DESCRIPTIONS,
        "phases": PHASE_DESCRIPTIONS,
        "libraries": LIBRARIES,
        "statistic": "Median and inclusive IQR of 20 Criterion sample means, 10 per fresh process round; not single-call p99.",
        "excluded": "Generation, file I/O and final result destruction. No Core IR lowering, hashing, reference resolution or runtime execution.",
        "yaml_policy": "Default-policy failures recorded separately. Measured YAML allows 5,000,000 nodes and 10,000,000 events; other guards unchanged (depth 64, scalar bytes 64 MiB).",
        "memory_note": "Maximum observed process working-set high-water mark across both rounds, sampled every 100 ms. Includes resident input, parser, allocator and harness; not incremental output size. Serialization also loads the common record tree. Peaks inside the final poll window may be missed.",
        "rows": rows,
    }
    (directory / "summary.json").write_text(
        json.dumps(summary, indent=2), encoding="utf-8"
    )
    with (directory / "summary.csv").open("w", newline="", encoding="utf-8") as stream:
        fields = [k for k in rows[0] if k not in ("samples_ms", "default_decode_error")]
        writer = csv.DictWriter(stream, fieldnames=fields, extrasaction="ignore")
        writer.writeheader()
        writer.writerows(rows)
    print(f"Validated {len(raw['cells'])} raw cells / {len(rows)} paired observations")
    for phase in PHASES:
        print(phase)
        for pattern in PATTERNS:
            values = {
                r["format"]: round(r["median_ms"], 3)
                for r in rows
                if r["phase"] == phase
                and r["pattern"] == pattern
                and r["nodes"] == 50000
            }
            print(pattern, values)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument(
        "--baseline",
        type=Path,
        help="Compare directory/results.json; non-pass exits nonzero",
    )
    parser.add_argument("--regression", type=float, default=0.10)
    parser.add_argument("--drift", type=float, default=0.10)
    args = parser.parse_args()
    if args.baseline is None:
        export(args.directory)
    else:
        try:
            gate = RegressionGate(args.regression, args.drift)
        except ValueError as error:
            parser.error(str(error))
        result = gate.run(args.baseline, args.directory / "results.json")
        report_path = args.directory / "regression.json"
        try:
            report_path.write_text(json.dumps(result, indent=2), encoding="utf-8")
        except OSError as error:
            result = {"status": "invalid", "error": f"cannot write report: {error}"}
        print(
            json.dumps(
                {
                    "status": result["status"],
                    "report": str(report_path),
                    "error": result.get("error"),
                }
            )
        )
        sys.exit(
            {"pass": 0, "regression": 1, "invalid": 2, "inconclusive": 3}[
                result["status"]
            ]
        )
