"""Run paired Criterion benchmarks and generate performance comparison reports."""

from __future__ import annotations

import argparse
import csv
import json
import math
import os
import re
import statistics
import subprocess
from pathlib import Path


def executables(build_log: Path) -> dict[str, str]:
    result = {}
    for line in build_log.read_text().splitlines():
        message = json.loads(line)
        if (
            message.get("reason") == "compiler-artifact"
            and "bench" in message["target"]["kind"]
            and message.get("executable")
        ):
            result[message["target"]["name"]] = message["executable"]
    if not result:
        raise ValueError(f"no benchmark executables in {build_log}")
    return result


def discover(binaries: dict[str, str], root: Path, pattern: str) -> dict[str, str]:
    selected = {}
    matcher = re.compile(pattern)
    for binary in binaries.values():
        output = subprocess.check_output(
            [binary, "--list"], cwd=root, text=True, timeout=120
        )
        for line in output.splitlines():
            if not line.endswith(": benchmark"):
                continue
            name = line.removesuffix(": benchmark")
            if matcher.search(name):
                if name in selected:
                    raise ValueError(f"duplicate benchmark ID: {name}")
                selected[name] = binary
    if not selected:
        raise ValueError(f"no benchmarks match {pattern!r} in {root}")
    return selected


def revision_order(round_index: int) -> tuple[str, str]:
    return ("baseline", "current") if round_index % 2 == 0 else ("current", "baseline")


def run(args: argparse.Namespace, parser: argparse.ArgumentParser) -> None:
    if args.rounds not in (2, 4, 6):
        parser.error("rounds must be 2, 4, or 6 to balance execution order")
    if args.sample_size < 10 or not (
        0 < args.warm_up_time <= 60 and 0 < args.measurement_time <= 60
    ):
        parser.error("sample size must be >= 10 and durations must be in (0, 60]")

    allowed = sorted(os.sched_getaffinity(0))
    cpu = args.cpu if args.cpu is not None else allowed[-1]
    if cpu not in allowed:
        parser.error(f"CPU {cpu} is outside allowed CPUs: {allowed}")
    # Inherited by discovery and all measured child processes. No cargo invocation
    # or compilation occurs after this point.
    os.sched_setaffinity(0, {cpu})
    roots = {
        side: getattr(args, f"{side}_root").resolve()
        for side in ("baseline", "current")
    }
    cases = {
        side: discover(
            executables(getattr(args, f"{side}_build")), roots[side], args.filter
        )
        for side in roots
    }
    common = sorted(cases["baseline"].keys() & cases["current"].keys())
    if not common:
        raise ValueError("no comparable benchmark IDs")
    report = args.report_dir.resolve()
    report.mkdir(parents=True, exist_ok=True)
    manifest = {
        "cpu": cpu,
        "rounds": args.rounds,
        "sample_size": args.sample_size,
        "warm_up_time": args.warm_up_time,
        "measurement_time": args.measurement_time,
        "filter": args.filter,
        "added": sorted(cases["current"].keys() - cases["baseline"].keys()),
        "removed": sorted(cases["baseline"].keys() - cases["current"].keys()),
        "cases": common,
        "runs": [],
    }
    manifest_path = report / "paired.json"
    # Refuse to mix old and new samples when rerunning locally.
    with manifest_path.open("x") as file:
        json.dump(manifest, file, indent=2)
    for index, name in enumerate(common):
        for round_index in range(args.rounds):
            for side in revision_order(round_index):
                directory = (
                    report / f"case-{index:03}" / f"round-{round_index + 1}" / side
                )
                directory.mkdir(parents=True, exist_ok=False)
                command = [
                    cases[side][name],
                    name,
                    "--exact",
                    "--bench",
                    "--noplot",
                    "--sample-size",
                    str(args.sample_size),
                    "--warm-up-time",
                    str(args.warm_up_time),
                    "--measurement-time",
                    str(args.measurement_time),
                ]
                run = {
                    "benchmark": name,
                    "round": round_index + 1,
                    "revision": side,
                    "directory": str(directory.relative_to(report)),
                    "command": command,
                    "status": "running",
                }
                manifest["runs"].append(run)
                manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
                print(
                    f"[{index + 1}/{len(common)}] round {round_index + 1} {side}: {name}",
                    flush=True,
                )
                with (directory / "console.log").open("w") as log:
                    try:
                        subprocess.run(
                            command,
                            cwd=roots[side],
                            check=True,
                            timeout=300,
                            env={**os.environ, "CRITERION_HOME": str(directory)},
                            stdout=log,
                            stderr=subprocess.STDOUT,
                        )
                    except (subprocess.SubprocessError, OSError):
                        run["status"] = "failed"
                        manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")
                        print((directory / "console.log").read_text(), flush=True)
                        raise
                run["status"] = "success"
                manifest_path.write_text(json.dumps(manifest, indent=2) + "\n")


def escape(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace("|", "&#124;")
        .replace("\n", " ")
        .replace("\r", " ")
        .replace("`", "&#96;")
    )


def read_result(directory: Path, expected_id: str) -> dict:
    paths = list(directory.rglob("new/benchmark.json"))
    if len(paths) != 1:
        raise ValueError(f"expected exactly one Criterion result in {directory}")
    path = paths[0]
    benchmark = json.loads(path.read_text())
    if benchmark["full_id"] != expected_id:
        raise ValueError(f"unexpected benchmark in {path}")
    estimates = json.loads(path.with_name("estimates.json").read_text())
    estimate = estimates.get("slope") or estimates["mean"]
    ci = estimate["confidence_interval"]
    point, lower, upper = (
        float(estimate["point_estimate"]),
        float(ci["lower_bound"]),
        float(ci["upper_bound"]),
    )
    if (
        not all(math.isfinite(x) and x > 0 for x in (point, lower, upper))
        or not lower <= point <= upper
    ):
        raise ValueError(f"invalid time estimate in {path}")
    sample = json.loads(path.with_name("sample.json").read_text())
    if len(sample["times"]) != len(sample["iters"]) or len(sample["times"]) < 2:
        raise ValueError(f"invalid samples in {path}")
    if not all(math.isfinite(x) and x > 0 for x in sample["times"] + sample["iters"]):
        raise ValueError(f"invalid samples in {path}")
    times = [t / n for t, n in zip(sample["times"], sample["iters"])]
    return {
        "point": point,
        "lower": lower,
        "upper": upper,
        "cv": statistics.stdev(times) / statistics.mean(times) * 100,
        "throughput": benchmark.get("throughput"),
    }


def summarize(pairs: list[tuple[dict, dict]], threshold: float) -> dict:
    changes = [(b["point"] / a["point"] - 1) * 100 for a, b in pairs]
    # These are ratios of each run's marginal CI bounds, not a calibrated 95%
    # interval for the change. Include all rounds rather than hiding drift.
    lower = min((b["lower"] / a["upper"] - 1) * 100 for a, b in pairs)
    upper = max((b["upper"] / a["lower"] - 1) * 100 for a, b in pairs)
    cv = max(result["cv"] for pair in pairs for result in pair)
    if cv > 5 or min(changes) < 0 < max(changes):
        status = "Unstable: remeasure"
    elif lower > threshold:
        status = "Possible slowdown"
    elif upper < -threshold:
        status = "Possible improvement"
    elif all(abs(change) <= threshold for change in changes):
        status = "Within threshold"
    else:
        status = "Uncertain: remeasure"
    return {
        "baseline_ns": statistics.median(a["point"] for a, _ in pairs),
        "current_ns": statistics.median(b["point"] for _, b in pairs),
        "change_pct": statistics.median(changes),
        "round_changes_pct": changes,
        "lower_pct": lower,
        "upper_pct": upper,
        "max_cv_pct": cv,
        "status": status,
    }


def generate(root: Path, threshold: float) -> tuple[str, list[dict]]:
    manifest = json.loads((root / "paired.json").read_text())
    if manifest["rounds"] not in (2, 4, 6) or len(set(manifest["cases"])) != len(
        manifest["cases"]
    ):
        raise ValueError("invalid paired run plan")
    expected = {
        (name, i, side)
        for name in manifest["cases"]
        for i in range(1, manifest["rounds"] + 1)
        for side in ("baseline", "current")
    }
    runs = {}
    for run in manifest["runs"]:
        key = (run["benchmark"], run["round"], run["revision"])
        if key not in expected or key in runs or run["status"] != "success":
            raise ValueError(f"duplicate or incomplete run: {key}")
        runs[key] = read_result(root / run["directory"], run["benchmark"])
    if runs.keys() != expected:
        raise ValueError(f"missing paired runs: {sorted(expected - runs.keys())}")
    rows, changed = [], []
    for name in manifest["cases"]:
        pairs = [
            (runs[name, i, "baseline"], runs[name, i, "current"])
            for i in range(1, manifest["rounds"] + 1)
        ]
        throughputs = [result["throughput"] for pair in pairs for result in pair]
        if any(value != throughputs[0] for value in throughputs):
            changed.append(name)
            continue
        rows.append({"benchmark": name, **summarize(pairs, threshold)})
    if not rows:
        raise ValueError("no comparable results with matching throughput")
    rows.sort(key=lambda row: row["change_pct"], reverse=True)
    lines = [
        "## Paired performance comparison",
        "",
        (
            f"CPU: {manifest['cpu']}. Paired rounds: {manifest['rounds']} (alternating AB / BA). "
            f"Samples/run: {manifest['sample_size']}; warm-up: {manifest['warm_up_time']}s; "
            f"measurement: {manifest['measurement_time']}s."
        ),
        "",
        (
            f"Compared: {len(rows)}. Added: {len(manifest['added'])}. "
            f"Removed: {len(manifest['removed'])}. Changed throughput: {len(changed)} (excluded)."
        ),
        "",
        (
            "Time change is the median of per-round (current / baseline - 1); positive means slower. "
            "Times are medians across rounds. Round changes show order sensitivity."
        ),
        "",
        (
            "The uncertainty envelope uses the most extreme ratios of Criterion CI bounds across rounds. "
            "It is descriptive, NOT a 95% confidence interval for the change and cannot detect all VM drift. "
            f"Candidate changes require the entire envelope beyond ±{threshold:g}% and sample CV ≤5%. "
            "Opposite round directions or CV >5% are marked unstable. This report does not fail the job."
        ),
        "",
        "| Benchmark | Baseline | Current | Time change | Round changes | Uncertainty envelope | Max sample CV | Assessment |",
        "| --- | ---: | ---: | ---: | --- | --- | ---: | --- |",
    ]
    for row in rows:
        changes = ", ".join(f"{value:+.2f}%" for value in row["round_changes_pct"])
        lines.append(
            f"| {escape(row['benchmark'])} | {row['baseline_ns'] / 1000:.3f} µs | "
            f"{row['current_ns'] / 1000:.3f} µs | {row['change_pct']:+.2f}% | {changes} | "
            f"[{row['lower_pct']:+.2f}%, {row['upper_pct']:+.2f}%] | "
            f"{row['max_cv_pct']:.2f}% | {row['status']} |"
        )
    for title, names in [
        ("Added (not timed)", manifest["added"]),
        ("Removed", manifest["removed"]),
        ("Changed throughput", changed),
    ]:
        if names:
            lines.extend(["", f"### {title}", ""])
            lines.extend(f"- {escape(name)}" for name in names)
    return "\n".join(lines) + "\n", rows


def write_report(args: argparse.Namespace, parser: argparse.ArgumentParser) -> None:
    if not math.isfinite(args.threshold) or args.threshold < 0:
        parser.error("threshold must be finite and nonnegative")
    markdown, rows = generate(args.report_dir, args.threshold)
    (args.report_dir / "comparison.md").write_text(markdown)
    with (args.report_dir / "comparison.csv").open("w", newline="") as file:
        writer = csv.DictWriter(file, fieldnames=list(rows[0]))
        writer.writeheader()
        for row in rows:
            writer.writerow(
                {**row, "round_changes_pct": json.dumps(row["round_changes_pct"])}
            )
    print(markdown, end="")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    run_parser = commands.add_parser(
        "run", help="measure prebuilt benchmarks on one CPU in alternating order"
    )
    for side in ("baseline", "current"):
        run_parser.add_argument(f"--{side}-build", type=Path, required=True)
        run_parser.add_argument(f"--{side}-root", type=Path, required=True)
    run_parser.add_argument("--report-dir", type=Path, required=True)
    run_parser.add_argument("--filter", required=True)
    run_parser.add_argument("--rounds", type=int, default=2)
    run_parser.add_argument("--sample-size", type=int, default=100)
    run_parser.add_argument("--warm-up-time", type=float, default=3)
    run_parser.add_argument("--measurement-time", type=float, default=5)
    run_parser.add_argument("--cpu", type=int)
    report_parser = commands.add_parser(
        "report", help="generate Markdown and CSV from saved paired results"
    )
    report_parser.add_argument("--report-dir", type=Path, required=True)
    report_parser.add_argument("--threshold", type=float, default=3)
    args = parser.parse_args()
    if args.command == "run":
        run(args, run_parser)
    else:
        write_report(args, report_parser)


if __name__ == "__main__":
    main()
