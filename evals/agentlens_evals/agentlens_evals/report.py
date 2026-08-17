"""Aggregation, falsification, and reports.

grade_campaign is the harness grade.py aggregation, ported: per-arm and
per-repetition quartiles, per-task head-to-head, and cost/accuracy
comparisons of agentlens against each control separately -- controls are
never pooled -- judged against the pre-registered thresholds in
protocol.json. The pydantic-evals passes in evaluation_reports are derived
artifacts on top of the same on-disk runs.
"""

from __future__ import annotations

import hashlib
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

from blake3 import blake3

from agentlens_evals import dataset as dataset_module
from agentlens_evals.dataset import build_dataset, load_tasks
from agentlens_evals.evaluators import BenchmarkScores
from agentlens_evals.grading import (
    MATCHER_VERSION,
    REFERENCE_ARM,
    grade_run,
    load_run,
    quartiles,
)
from agentlens_evals.paths import HARNESS_ROOT


def grade_campaign(
    runs_root: Path, repetitions: int, arms: list[str]
) -> dict[str, Any]:
    if REFERENCE_ARM not in arms:
        raise SystemExit(f"{REFERENCE_ARM} must be among the graded arms")
    task_bytes = dataset_module.verify_gold()
    tasks = [task.model_dump() for task in load_tasks()]
    repetition_range = range(1, repetitions + 1)

    runs: list[dict[str, Any]] = []
    for repetition in repetition_range:
        for arm in arms:
            for task in tasks:
                run_dir = runs_root / f"repetition-{repetition}" / arm / task["id"]
                graded = grade_run(task, arm, run_dir)
                graded["repetition"] = repetition
                runs.append(graded)

    grouped: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for run in runs:
        grouped[(run["arm"], run["task_id"])].append(run)
    per_task: list[dict[str, Any]] = []
    for task in tasks:
        row: dict[str, Any] = {"task_id": task["id"]}
        for arm in arms:
            arm_runs = grouped[(arm, task["id"])]
            row[arm] = {
                "score": quartiles([r["score"] for r in arm_runs]),
                "tool_result_tokens": quartiles(
                    [float(r["tool_result_tokens"]) for r in arm_runs]
                ),
                "navigation": quartiles(
                    [
                        float(r["navigation"] if r["navigation"] is not None else 26)
                        for r in arm_runs
                    ]
                ),
                "capped_runs": sum(bool(r["capped"]) for r in arm_runs),
            }
        per_task.append(row)

    repetition_metrics: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for arm in arms:
        for repetition in repetition_range:
            arm_runs = [
                run
                for run in runs
                if run["arm"] == arm and run["repetition"] == repetition
            ]
            total_points = sum(run["score"] for run in arm_runs)
            total_tokens = sum(run["tool_result_tokens"] for run in arm_runs)
            repetition_metrics[arm].append(
                {
                    "repetition": repetition,
                    "accuracy": statistics.mean(run["score"] for run in arm_runs),
                    "cost_tokens_per_point": (
                        total_tokens / total_points if total_points else None
                    ),
                    "navigation_median": statistics.median(
                        run["navigation"] if run["navigation"] is not None else 26
                        for run in arm_runs
                    ),
                    "total_points": total_points,
                    "total_tool_result_tokens": total_tokens,
                    "capped_runs": sum(bool(run["capped"]) for run in arm_runs),
                }
            )
    arm_summary: dict[str, Any] = {}
    for arm in arms:
        metrics = repetition_metrics[arm]
        arm_summary[arm] = {
            "accuracy": quartiles([metric["accuracy"] for metric in metrics]),
            "cost_tokens_per_point": quartiles(
                [metric["cost_tokens_per_point"] for metric in metrics]
            ),
            "navigation": quartiles(
                [metric["navigation_median"] for metric in metrics]
            ),
            "repetitions": metrics,
            "capped_runs": sum(metric["capped_runs"] for metric in metrics),
        }

    protocol = json.loads((HARNESS_ROOT / "protocol.json").read_text())
    thresholds = protocol["falsification"]
    comparisons: dict[str, Any] = {}
    for control in [arm for arm in arms if arm != REFERENCE_ARM]:
        head_to_head = {"agentlens_wins": 0, "control_wins": 0, "ties": 0}
        for row in per_task:
            left = row[REFERENCE_ARM]["score"]["median"]
            right = row[control]["score"]["median"]
            if left > right:
                head_to_head["agentlens_wins"] += 1
            elif left < right:
                head_to_head["control_wins"] += 1
            else:
                head_to_head["ties"] += 1
        indices = range(len(repetition_range))
        cost_ratio = quartiles(
            [
                repetition_metrics[REFERENCE_ARM][index]["cost_tokens_per_point"]
                / repetition_metrics[control][index]["cost_tokens_per_point"]
                for index in indices
            ]
        )
        accuracy_delta = quartiles(
            [
                repetition_metrics[REFERENCE_ARM][index]["accuracy"]
                - repetition_metrics[control][index]["accuracy"]
                for index in indices
            ]
        )
        limits = thresholds.get(f"vs_{control}", {})
        failures = {
            "cost_ratio_too_high": cost_ratio["median"] >= limits["maximum_cost_ratio"],
            "accuracy_below_control": (
                accuracy_delta["median"] < -limits["maximum_accuracy_delta_below_arm"]
            ),
            "too_many_tasks_lost": (
                head_to_head["control_wins"] > limits["maximum_tasks_lost"]
            ),
        }
        comparisons[control] = {
            "cost_ratio": cost_ratio,
            "accuracy_delta": accuracy_delta,
            "head_to_head": head_to_head,
            "thresholds": limits,
            "falsification": failures,
            "failed": any(failures.values()),
        }

    return {
        "subject": protocol["subject"],
        "matcher_version": MATCHER_VERSION,
        "repetitions_graded": list(repetition_range),
        "arms_graded": list(arms),
        "gold_blake3": blake3(task_bytes).hexdigest(),
        "gold_sha256": hashlib.sha256(task_bytes).hexdigest(),
        "arms": arm_summary,
        "comparisons": comparisons,
        "benchmark_failed": any(entry["failed"] for entry in comparisons.values()),
        "per_task": per_task,
        "runs": runs,
    }


def write_results(results: dict[str, Any], runs_root: Path) -> Path:
    path = runs_root / "results.json"
    path.write_text(
        json.dumps(results, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return path


def render_markdown(results: dict[str, Any]) -> str:
    lines = [
        f"# Benchmark results — {results['subject']}",
        "",
        (
            f"Matcher v{results['matcher_version']}; "
            f"repetitions {results['repetitions_graded']}; "
            f"benchmark_failed: **{results['benchmark_failed']}**"
        ),
        "",
        "| arm | accuracy (median) | tokens/point (median) | navigation (median) | capped |",
        "|---|---|---|---|---|",
    ]
    for arm, summary in results["arms"].items():
        lines.append(
            f"| {arm} | {summary['accuracy']['median']:.3f} "
            f"| {summary['cost_tokens_per_point']['median']:.0f} "
            f"| {summary['navigation']['median']:.1f} "
            f"| {summary['capped_runs']} |"
        )
    lines.append("")
    for control, entry in results["comparisons"].items():
        h2h = entry["head_to_head"]
        lines += [
            f"## agentlens vs {control}",
            "",
            f"- cost ratio median: {entry['cost_ratio']['median']:.3f}",
            f"- accuracy delta median: {entry['accuracy_delta']['median']:+.3f}",
            f"- head-to-head: {h2h['agentlens_wins']}W / {h2h['control_wins']}L / {h2h['ties']}T",
            f"- falsified: {entry['failed']}",
            "",
        ]
    return "\n".join(lines)


def evaluation_reports(runs_root: Path, repetitions: int, arms: list[str]):
    """Derived pydantic-evals reports, one evaluate pass per arm x repetition."""
    tasks = load_tasks()
    reports = []
    for repetition in range(1, repetitions + 1):
        for arm in arms:
            built = build_dataset(tasks, arm=arm, repetition=repetition)
            built.add_evaluator(BenchmarkScores())
            report = built.evaluate_sync(
                lambda run_id: load_run(run_id, runs_root),
                name=f"r{repetition}-{arm}",
                progress=False,
            )
            reports.append(report)
    return reports
