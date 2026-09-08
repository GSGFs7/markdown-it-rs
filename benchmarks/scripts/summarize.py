#!/usr/bin/env python3
"""Turn Criterion JSON results into a compact Markdown table or CSV."""

from __future__ import annotations

import argparse
import csv
import io
import json
import sys
from dataclasses import dataclass
from pathlib import Path

DEFAULT_BASELINE = "markdown-it-rs"
PHASE_ORDER = {
    "parse": 0,
    "render": 1,
    "parse-render": 2,
    "document-parse": 3,
    "document-render": 4,
    "document-end-to-end": 5,
}
CORPUS_ORDER = {
    "small-real-world": 0,
    "commonmark-emphasis": 1,
    "commonmark-spec": 2,
    "plain-text": 3,
    "marker-heavy": 4,
    "unicode-heavy": 5,
}
ENGINE_ORDER = {
    "markdown-it-rs": 0,
    "markdown-it-rs-0.7": 1,
    "markdown-it-0.6": 2,
    "comrak-0.52": 3,
    "pulldown-cmark-0.13": 4,
    "markdown-rs-1.0": 5,
    "legacy-tree": 6,
    "legacy-arena-bridge": 7,
    "arena-direct": 8,
    "arena-bridge": 9,
}


@dataclass(frozen=True)
class Result:
    phase: str
    configuration: str | None
    corpus: str
    engine: str
    time_ns: float
    lower_ns: float
    upper_ns: float
    input_bytes: int | None

    @property
    def throughput_mib_s(self) -> float | None:
        if self.input_bytes is None or self.time_ns <= 0:
            return None
        return self.input_bytes * 1_000_000_000 / self.time_ns / (1024 * 1024)


def parse_args() -> argparse.Namespace:
    benchmark_root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(
        description="Summarize Criterion benchmark JSON without parsing console logs."
    )
    parser.add_argument(
        "--criterion-dir",
        type=Path,
        default=benchmark_root / "target" / "criterion",
        help="Criterion result directory (default: benchmarks/target/criterion)",
    )
    parser.add_argument(
        "--dataset",
        default="new",
        help="Criterion dataset directory to read, such as new or base (default: new)",
    )
    parser.add_argument(
        "--baseline-engine",
        default=DEFAULT_BASELINE,
        help=f"engine used for relative speed (default: {DEFAULT_BASELINE})",
    )
    parser.add_argument(
        "--configuration",
        help="only include one document-parse configuration",
    )
    parser.add_argument(
        "--format",
        choices=("markdown", "csv"),
        default="markdown",
        help="output format (default: markdown)",
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        help="write to this file instead of stdout",
    )
    return parser.parse_args()


def read_json(path: Path) -> dict:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise RuntimeError(f"cannot read {path}: {error}") from error


def load_results(root: Path, dataset: str) -> list[Result]:
    if Path(dataset).name != dataset or dataset in {"", ".", ".."}:
        raise RuntimeError(f"invalid dataset name: {dataset!r}")
    if not root.is_dir():
        raise RuntimeError(f"Criterion directory does not exist: {root}")

    results = []
    for benchmark_path in root.rglob("benchmark.json"):
        if benchmark_path.parent.name != dataset:
            continue

        estimates_path = benchmark_path.with_name("estimates.json")
        if not estimates_path.is_file():
            continue

        benchmark = read_json(benchmark_path)
        parts = benchmark.get("full_id", "").split("/")
        if len(parts) == 3 and parts[0] in PHASE_ORDER:
            phase, corpus, engine = parts
            configuration = None
        elif len(parts) == 4 and parts[0] == "document-parse":
            phase, configuration, corpus, engine = parts
        else:
            # Ignore old flat benchmark IDs and unrelated Criterion data.
            continue

        estimates = read_json(estimates_path)
        typical = estimates.get("slope") or estimates.get("mean")
        if not typical:
            raise RuntimeError(f"no slope or mean estimate in {estimates_path}")

        confidence = typical["confidence_interval"]
        throughput = benchmark.get("throughput")
        input_bytes = throughput.get("Bytes") if isinstance(throughput, dict) else None
        results.append(
            Result(
                phase=phase,
                configuration=configuration,
                corpus=corpus,
                engine=engine,
                time_ns=float(typical["point_estimate"]),
                lower_ns=float(confidence["lower_bound"]),
                upper_ns=float(confidence["upper_bound"]),
                input_bytes=int(input_bytes) if input_bytes is not None else None,
            )
        )

    if not results:
        raise RuntimeError(f"no structured benchmark results found in {root}")

    results.sort(key=result_sort_key)
    return results


def result_sort_key(result: Result) -> tuple:
    corpus_key = CORPUS_ORDER.get(result.corpus, len(CORPUS_ORDER))
    engine_key = ENGINE_ORDER.get(result.engine, len(ENGINE_ORDER))
    return (
        corpus_key,
        result.corpus,
        PHASE_ORDER[result.phase],
        result.configuration or "",
        engine_key,
        result.engine,
    )


def format_duration(nanoseconds: float) -> str:
    if nanoseconds < 1_000:
        return f"{nanoseconds:.2f} ns"
    if nanoseconds < 1_000_000:
        return f"{nanoseconds / 1_000:.2f} µs"
    if nanoseconds < 1_000_000_000:
        return f"{nanoseconds / 1_000_000:.2f} ms"
    return f"{nanoseconds / 1_000_000_000:.2f} s"


def markdown_report(results: list[Result], baseline_engine: str, dataset: str) -> str:
    baselines = {
        (result.configuration, result.corpus, result.phase): result.time_ns
        for result in results
        if result.engine == baseline_engine
    }
    corpora = []
    for result in results:
        if result.corpus not in corpora:
            corpora.append(result.corpus)

    lines = [
        "# Criterion benchmark summary",
        "",
        f"Dataset: `{dataset}`. Relative speed baseline: `{baseline_engine}`.",
        "",
    ]
    for corpus in corpora:
        lines.extend(
            [
                f"## {corpus}",
                "",
                f"| Phase | Configuration | Engine | Time | 95% CI | Throughput | vs `{baseline_engine}` |",
                "| --- | --- | --- | ---: | ---: | ---: | ---: |",
            ]
        )
        for result in results:
            if result.corpus != corpus:
                continue
            throughput = result.throughput_mib_s
            throughput_text = "—" if throughput is None else f"{throughput:.2f} MiB/s"
            baseline = baselines.get(
                (result.configuration, result.corpus, result.phase)
            )
            relative = "—" if baseline is None else f"{baseline / result.time_ns:.2f}×"
            configuration = result.configuration or "—"
            confidence = (
                f"{format_duration(result.lower_ns)}–{format_duration(result.upper_ns)}"
            )
            lines.append(
                f"| {result.phase} | {configuration} | {result.engine} | "
                f"{format_duration(result.time_ns)} | {confidence} | "
                f"{throughput_text} | {relative} |"
            )
        lines.append("")

    return "\n".join(lines)


def csv_report(results: list[Result], baseline_engine: str) -> str:
    baselines = {
        (result.configuration, result.corpus, result.phase): result.time_ns
        for result in results
        if result.engine == baseline_engine
    }
    output = io.StringIO(newline="")
    writer = csv.writer(output)
    writer.writerow(
        (
            "corpus",
            "phase",
            "configuration",
            "engine",
            "time_ns",
            "lower_ns",
            "upper_ns",
            "input_bytes",
            "throughput_mib_s",
            "relative_speed",
        )
    )
    for result in results:
        baseline = baselines.get(
            (result.configuration, result.corpus, result.phase)
        )
        writer.writerow(
            (
                result.corpus,
                result.phase,
                "" if result.configuration is None else result.configuration,
                result.engine,
                f"{result.time_ns:.6f}",
                f"{result.lower_ns:.6f}",
                f"{result.upper_ns:.6f}",
                "" if result.input_bytes is None else result.input_bytes,
                ""
                if result.throughput_mib_s is None
                else f"{result.throughput_mib_s:.6f}",
                "" if baseline is None else f"{baseline / result.time_ns:.6f}",
            )
        )
    return output.getvalue()


def main() -> int:
    args = parse_args()
    try:
        results = load_results(args.criterion_dir, args.dataset)
        if args.configuration:
            results = [
                result
                for result in results
                if result.configuration == args.configuration
            ]
            if not results:
                raise RuntimeError(
                    f"no results for configuration: {args.configuration!r}"
                )
        report = (
            markdown_report(results, args.baseline_engine, args.dataset)
            if args.format == "markdown"
            else csv_report(results, args.baseline_engine)
        )
        if args.output:
            args.output.write_text(report, encoding="utf-8")
        else:
            sys.stdout.write(report)
        return 0
    except (OSError, RuntimeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
