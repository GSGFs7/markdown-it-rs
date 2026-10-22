"""Compare matching Criterion benchmark IDs across two revisions."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


def load_results(root: Path) -> dict[str, tuple[float, object]]:
    results = {}
    for path in root.rglob("new/benchmark.json"):
        benchmark = json.loads(path.read_text(encoding="utf-8"))
        estimates = json.loads(
            path.with_name("estimates.json").read_text(encoding="utf-8")
        )
        estimate = estimates.get("slope") or estimates["mean"]
        time_ns = float(estimate["point_estimate"])
        benchmark_id = benchmark["full_id"]
        if not math.isfinite(time_ns) or time_ns <= 0:
            raise ValueError(f"invalid time estimate for {benchmark_id}: {time_ns}")
        if benchmark_id in results:
            raise ValueError(f"duplicate benchmark ID: {benchmark_id}")
        results[benchmark_id] = (time_ns, benchmark.get("throughput"))
    if not results:
        raise ValueError(f"no Criterion results in {root}")
    return results


def escape(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace("|", "&#124;")
        .replace("\n", " ")
        .replace("\r", " ")
        .replace("`", "&#96;")
    )


def compare_report(baseline: dict, current: dict) -> str:
    common = baseline.keys() & current.keys()
    matched = [key for key in common if baseline[key][1] == current[key][1]]
    changed = common - set(matched)
    added = current.keys() - baseline.keys()
    removed = baseline.keys() - current.keys()
    lines = [
        "## Changes against baseline",
        "",
        (
            f"Compared: {len(matched)}. Added: {len(added)}. Removed: {len(removed)}. "
            f"Changed input size/throughput: {len(changed)} (excluded)."
        ),
        "",
        (
            "Time change = (Current / Baseline - 1) × 100%. "
            "Positive (+) means more time taken (slower); "
            "negative (-) means less time taken (faster). Lower times are better."
        ),
        "",
        "| Benchmark | Baseline | Current | Time change |",
        "| --- | ---: | ---: | ---: |",
    ]
    # Show the largest slowdowns first, followed by improvements.
    for key in sorted(
        matched, key=lambda key: current[key][0] / baseline[key][0], reverse=True
    ):
        before, after = baseline[key][0], current[key][0]
        delta = (after / before - 1) * 100
        lines.append(
            f"| {escape(key)} | {before / 1000:.3f} µs | "
            f"{after / 1000:.3f} µs | {delta:+.2f}% |"
        )
    if not matched:
        lines.extend(["", "No comparable benchmark IDs were found."])
    for title, keys in [
        ("Added", added),
        ("Removed", removed),
        ("Changed input size/throughput", changed),
    ]:
        if keys:
            lines.extend(["", f"### {title}", ""])
            lines.extend(f"- {escape(key)}" for key in sorted(keys))
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-dir", type=Path, required=True)
    parser.add_argument("--current-dir", type=Path, required=True)
    args = parser.parse_args()
    print(
        compare_report(load_results(args.baseline_dir), load_results(args.current_dir)),
        end="",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
