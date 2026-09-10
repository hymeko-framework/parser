"""Run certified Criterion format cells with the existing Core memory harness.

Windows runner; library parsing remains entirely in the Rust benchmark. Timing
samples exclude fixture generation, file reads and final output destruction.
"""

from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import platform
import statistics
import subprocess
import time
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path
from typing import Any

PATTERNS = ("flat", "linked", "unicode", "long_text", "nested")
FORMATS = ("hymeko", "json", "xml", "yaml")
PHASES = ("serialize", "parse_document", "decode_records")


@dataclass(frozen=True)
class Cell:
    pattern: str
    nodes: int
    format: str
    phase: str
    round: int

    @property
    def key(self) -> str:
        return f"r{self.round}-{self.pattern}-{self.nodes}-{self.format}-{self.phase}"


class Campaign:
    def __init__(
        self,
        args: argparse.Namespace,
        invoker: Callable[[str, dict[str, str], str], dict[str, Any]] | None = None,
    ):
        self.args = args
        self.invoker = invoker
        self.root = Path(__file__).resolve().parents[1]
        self.output = Path(args.output).resolve()
        self.output.mkdir(parents=True, exist_ok=True)
        self.fixtures = (
            Path(args.fixtures).resolve() if args.fixtures else self.output / "fixtures"
        )
        self.binary = Path(args.binary).resolve()
        self.binary_hash = hashlib.sha256(self.binary.read_bytes()).hexdigest()
        self.memory_harness = (
            self.root.parent / "core/scripts/dev/measure_peak_memory.ps1"
        )
        if invoker is None and (os.name != "nt" or not self.memory_harness.is_file()):
            raise RuntimeError(
                "This runner requires Windows and the existing sibling Core memory harness"
            )

    @staticmethod
    def save(path: Path, value: Any) -> None:
        temporary = path.with_suffix(path.suffix + ".tmp")
        temporary.write_text(json.dumps(value, indent=2), encoding="utf-8")
        temporary.replace(path)

    def invoke(
        self, key: str, environment: dict[str, str], arguments: str
    ) -> dict[str, Any]:
        directory = self.output / "cells" / key
        directory.mkdir(parents=True, exist_ok=True)
        # Optional platform adapter must preserve resource caps and return the
        # standard result schema; run_cell applies the same acceptance checks.
        if self.invoker is not None:
            return self.invoker(key, environment, arguments)
        prefix = directory / "process"

        def quote(value: str | Path) -> str:
            return "'" + str(value).replace("'", "''") + "'"

        script = (
            f"& {quote(self.memory_harness)} -Executable {quote(self.binary)} "
            f"-ArgumentString {quote(arguments)} -WorkingDirectory {quote(self.root)} "
            f"-LogPrefix {quote(prefix)}"
        )
        if self.args.affinity_mask:
            script = (
                f"[Diagnostics.Process]::GetCurrentProcess().ProcessorAffinity = [IntPtr]{self.args.affinity_mask}\n"
                + script
            )
        command = [
            "powershell",
            "-NoProfile",
            "-EncodedCommand",
            base64.b64encode(script.encode("utf-16le")).decode("ascii"),
        ]
        environment = {
            **os.environ,
            **environment,
            "FORMAT_FIXTURES": str(self.fixtures),
            "CRITERION_HOME": str(directory / "criterion"),
            "RAYON_NUM_THREADS": "1",
        }
        start = time.monotonic()
        with (directory / "monitor.log").open("w", encoding="utf-8") as log:
            process = subprocess.Popen(
                command, env=environment, stdout=log, stderr=subprocess.STDOUT
            )
            timed_out = False
            try:
                process.wait(timeout=self.args.timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                # Only the process tree created immediately above is terminated.
                subprocess.run(
                    ["taskkill", "/PID", str(process.pid), "/T", "/F"],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                    check=False,
                )
                process.wait(timeout=10)
        metrics_path = prefix.with_suffix(".memory.json")
        metrics = (
            json.loads(metrics_path.read_text(encoding="utf-8-sig"))
            if metrics_path.exists()
            else {}
        )
        return {
            "command": command,
            "powershell_script": script,
            "pid": process.pid,
            "wall_seconds": time.monotonic() - start,
            "timeout": timed_out,
            "monitor_exit": process.returncode,
            "resources": metrics,
            "binary_sha256": self.binary_hash,
            "affinity_mask": self.args.affinity_mask,
            "directory": str(directory),
        }

    def prepare(self) -> None:
        for pattern in self.args.patterns:
            for nodes in self.args.sizes:
                result = self.invoke(
                    f"prepare-{pattern}-{nodes}",
                    {
                        "FORMAT_MODE": "prepare",
                        "FORMAT_PATTERN": pattern,
                        "FORMAT_NODES": str(nodes),
                    },
                    "--bench",
                )
                self.save(
                    self.output
                    / "cells"
                    / f"prepare-{pattern}-{nodes}"
                    / "result.json",
                    result,
                )
                if result["timeout"] or result["resources"].get("exit_code") != 0:
                    raise RuntimeError(
                        f"Fixture certification failed: {pattern}/{nodes}; see {result['directory']}"
                    )
                print(
                    f"CERTIFIED {pattern}/{nodes}: 4 exact round trips; {result['wall_seconds']:.2f}s",
                    flush=True,
                )

    def run_cell(self, cell: Cell, smoke: bool = False) -> dict[str, Any]:
        fixture_rows = json.loads(
            (self.fixtures / f"{cell.pattern}-{cell.nodes}.manifest.json").read_text()
        )
        fixture = next(f for f in fixture_rows if f["format"] == cell.format)
        source_hash = hashlib.sha256(
            (self.fixtures / fixture["file"]).read_bytes()
        ).hexdigest()
        output_path = self.output / "cells" / cell.key / "result.json"
        if output_path.exists() and not smoke:
            cached: dict[str, Any] = json.loads(output_path.read_text())
            if (
                cached.get("status") == "ok"
                and cached.get("binary_sha256") == self.binary_hash
                and cached.get("source_sha256") == source_hash
                and cached.get("affinity_mask") == self.args.affinity_mask
            ):
                return cached
        result = self.invoke(
            cell.key,
            {
                "FORMAT_MODE": "bench",
                "FORMAT_PATTERN": cell.pattern,
                "FORMAT_NODES": str(cell.nodes),
                "FORMAT_FORMAT": cell.format,
                "FORMAT_PHASE": cell.phase,
            },
            "--bench --noplot" + (" --test" if smoke else ""),
        )
        result.update(
            {"cell": vars(cell), "fixture": fixture, "source_sha256": source_hash}
        )
        metrics = result["resources"]
        result["status"] = (
            "ok"
            if not result["timeout"]
            and metrics.get("exit_code") == 0
            and metrics.get("cap_ok")
            and metrics.get("peak_observed")
            else "failed"
        )
        if result["status"] == "ok" and not smoke:
            sample_root = Path(result["directory"]) / "criterion"
            # Criterion can write paths longer than MAX_PATH; Python's glob
            # otherwise silently misses the deeper files on this Windows host.
            if os.name == "nt" and not str(sample_root).startswith("\\\\?\\"):
                absolute = str(sample_root.resolve())
                native = "UNC\\" + absolute[2:] if absolute.startswith("\\\\") else absolute
                sample_root = Path("\\\\?\\" + native)
            samples = list(sample_root.glob("**/new/sample.json"))
            if len(samples) != 1:
                raise RuntimeError(
                    f"Expected one Criterion sample set for {cell.key}: {samples}"
                )
            raw = json.loads(samples[0].read_text())
            values = [
                t / n / 1e6 for t, n in zip(raw["times"], raw["iters"], strict=True)
            ]
            if len(values) < 10 or min(values) <= 0:
                raise RuntimeError(f"Invalid sample set: {cell.key}")
            q1, _, q3 = statistics.quantiles(values, n=4, method="inclusive")
            median = statistics.median(values)
            result["measurement"] = {
                "median_ms": median,
                "q1_ms": q1,
                "q3_ms": q3,
                "iqr_ms": q3 - q1,
                "worst_ms": max(values),
                "samples_ms": values,
                "criterion": raw,
                "mib_per_second": fixture["bytes"] / (1024**2) / (median / 1000),
                "records_per_second": cell.nodes / (median / 1000),
            }
        self.save(output_path, result)
        return result

    def run(self) -> None:
        cells = [
            Cell(p, n, f, s, r)
            for r in range(1, self.args.rounds + 1)
            for p in self.args.patterns
            for n in self.args.sizes
            for f in self.args.formats
            for s in self.args.phases
        ]
        # Reverse round two to expose order/thermal drift instead of assigning it to a format.
        cells = [c for c in cells if c.round % 2] + list(
            reversed([c for c in cells if not c.round % 2])
        )
        if self.args.mode == "smoke":
            cells = [
                Cell(p, max(self.args.sizes), f, s, 0)
                for p in ("linked", "long_text")
                for f in self.args.formats
                for s in self.args.phases
                if s != "serialize"
            ]
        start = time.monotonic()
        for i, cell in enumerate(cells, 1):
            result = self.run_cell(cell, smoke=self.args.mode == "smoke")
            elapsed = time.monotonic() - start
            peak = result["resources"].get("peak_working_set_mib", 0)
            print(
                f"CELL {i}/{len(cells)} {cell.key} {result['status']} peak={peak:.1f}MiB elapsed={elapsed:.1f}s ETA={(len(cells) - i) * elapsed / i:.1f}s",
                flush=True,
            )
            if self.args.mode == "smoke" and result["status"] != "ok":
                raise RuntimeError(f"Smoke failed: {cell.key}")
        self.summarize()

    def summarize(self) -> None:
        rows = [
            json.loads(p.read_text())
            for p in sorted((self.output / "cells").glob("r[12]-*/result.json"))
        ]
        self.save(
            self.output / "results.json",
            {
                "platform": platform.platform(),
                "machine": platform.machine(),
                "host_id": platform.node(),
                "affinity_mask": self.args.affinity_mask,
                "binary_sha256": self.binary_hash,
                "cells": rows,
                "boundaries": {
                    "serialize": "Common owned records to resident UTF-8 text; generation excluded",
                    "parse_document": "Resident UTF-8 text to native AST / XML DOM / generic JSON or YAML value",
                    "decode_records": "Resident UTF-8 text to identical owned records; no semantic IR hashing or execution",
                },
            },
        )
        print(
            f"SAVED {len(rows)} measured cells to {self.output / 'results.json'}",
            flush=True,
        )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("prepare", "smoke", "run", "summarize"))
    parser.add_argument("--binary", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument(
        "--fixtures", help="Reuse an existing certified fixture directory"
    )
    parser.add_argument("--affinity-mask", type=lambda s: int(s, 0), default=None)
    parser.add_argument("--formats", nargs="+", choices=FORMATS, default=list(FORMATS))
    parser.add_argument("--phases", nargs="+", choices=PHASES, default=list(PHASES))
    parser.add_argument(
        "--patterns", nargs="+", choices=PATTERNS, default=list(PATTERNS)
    )
    parser.add_argument(
        "--sizes", nargs="+", type=int, default=[100, 1000, 10000, 50000]
    )
    parser.add_argument("--rounds", type=int, choices=(1, 2), default=2)
    parser.add_argument("--timeout", type=float, default=180)
    args = parser.parse_args()
    if not all(0 < n <= 50000 for n in args.sizes):
        parser.error("sizes must be between 1 and 50000")
    if args.affinity_mask is not None and not 0 < args.affinity_mask < 2**63:
        parser.error("affinity-mask must be a positive 63-bit mask")
    campaign = Campaign(args)
    if args.mode == "prepare":
        campaign.prepare()
    elif args.mode == "summarize":
        campaign.summarize()
    else:
        campaign.run()


if __name__ == "__main__":
    main()
