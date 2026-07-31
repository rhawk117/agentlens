#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import re
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

from blake3 import blake3

from paths import DJANGO_ROOT, ROOT, RUNS_ROOT

TASKS = json.loads((ROOT / "tasks.json").read_text(encoding="utf-8"))["tasks"]
HEDGES = ("might", "possibly", "i'm not sure", "i am not sure", "appears to", "may")


def normalize(text: str) -> str:
    return " ".join(text.casefold().split())


def is_negated(text: str, position: int) -> bool:
    prefix = text[max(0, position - 48) : position]
    return bool(
        re.search(
            r"(?:\bnot\b|\bnever\b|\bno\b|\bdoesn't\b|\bdoes not\b|\bisn't\b|\bis not\b)"
            r"(?:\W+\w+){0,4}\W*$",
            prefix,
        )
    )


def contains_asserted(text: str, candidate: str) -> bool:
    for match in re.finditer(re.escape(candidate), text):
        if not is_negated(text, match.start()):
            return True
    return False


def selector_name(address: str) -> str:
    selector = address.split("#", 1)[1]
    if selector.startswith("L"):
        return ""
    return selector.rsplit(".", 1)[-1]


def address_found(answer: str, gold: dict[str, Any]) -> bool:
    normalized = normalize(answer.replace("\\", "/"))
    address = normalize(gold["address"])
    if re.search(rf"{re.escape(address)}(?![\w.])", normalized):
        return True
    path, _selector = gold["address"].split("#", 1)
    path_norm = normalize(path)
    for match in re.finditer(re.escape(path_norm), normalized):
        window = normalized[max(0, match.start() - 100) : match.end() + 100]
        name = normalize(selector_name(gold["address"]))
        if name and re.search(rf"(?<![\w]){re.escape(name)}(?![\w])", window):
            return True
        for line_match in re.finditer(
            r"(?:\bL(\d+)(?:\s*-\s*L?(\d+))?\b|:(\d+)(?:-(\d+))?)",
            window,
            flags=re.IGNORECASE,
        ):
            start = int(line_match.group(1) or line_match.group(3))
            end = int(line_match.group(2) or line_match.group(4) or start)
            if start <= int(gold["end_line"]) and end >= int(gold["start_line"]):
                return True
    return False


def forbidden_penalty(answer: str, claims: list[str]) -> tuple[float, list[str]]:
    normalized = normalize(answer)
    penalty = 0.0
    matched: list[str] = []
    for claim in claims:
        needle = normalize(claim)
        positions = [
            match.start()
            for match in re.finditer(re.escape(needle), normalized)
            if not is_negated(normalized, match.start())
        ]
        if not positions:
            continue
        position = positions[0]
        matched.append(claim)
        window = normalized[max(0, position - 80) : position + len(needle) + 20]
        penalty += 0.1 if any(hedge in window for hedge in HEDGES) else 0.25
    return penalty, matched


def source_lines(gold: dict[str, Any]) -> set[str]:
    path = DJANGO_ROOT / gold["address"].split("#", 1)[0]
    lines = path.read_text(encoding="utf-8").splitlines()
    start = int(gold["start_line"]) - 1
    end = int(gold["end_line"])
    return {line.strip() for line in lines[start:end] if len(line.strip()) >= 8}


def call_retrieves(record: dict[str, Any], arm: str, golds: list[dict[str, Any]]) -> bool:
    if int(record["exit_code"]) not in {0, 1}:
        return False
    output = record["stdout"] + record["stderr"]
    if arm == "agentlens":
        return any(address_found(output, gold) for gold in golds)
    tool = record["tool"]
    args = record["args"]
    if tool == "cat":
        if int(record["exit_code"]) != 0 or not record["stdout"]:
            return False
        requested = {arg.replace("\\", "/") for arg in args}
        return any(gold["address"].split("#", 1)[0] in requested for gold in golds)
    normalized_output_lines = {
        re.sub(r"^.*?(?::\d+)?:", "", line).strip() for line in output.splitlines() if line.strip()
    }
    for gold in golds:
        if address_found(output, gold):
            return True
        gold_path = gold["address"].split("#", 1)[0]
        for line in output.splitlines():
            if not line.startswith(f"{gold_path}:"):
                continue
            rest = line[len(gold_path) + 1 :]
            numbered = re.match(r"(\d+):", rest)
            if numbered:
                line_number = int(numbered.group(1))
                if int(gold["start_line"]) <= line_number <= int(gold["end_line"]):
                    return True
            elif rest.strip() in source_lines(gold):
                return True
        explicitly_searched = gold_path in {arg.replace("\\", "/") for arg in args}
        substantial_source_lines = {line for line in source_lines(gold) if len(line) >= 20}
        if explicitly_searched and normalized_output_lines & substantial_source_lines:
            return True
    return False


def grade_run(task: dict[str, Any], arm: str, run_dir: Path) -> dict[str, Any]:
    answer_path = run_dir / "answer.txt"
    if not answer_path.exists():
        raise FileNotFoundError(f"missing answer: {answer_path}")
    answer = answer_path.read_text(encoding="utf-8")
    required_found = [
        gold["address"] for gold in task["required_addresses"] if address_found(answer, gold)
    ]
    supporting_found = [
        gold["address"] for gold in task["supporting_addresses"] if address_found(answer, gold)
    ]
    address_score = min(
        1.0,
        (len(required_found) + 0.5 * len(supporting_found)) / len(task["required_addresses"]),
    )
    normalized_answer = normalize(answer)
    facts_found = [
        fact["id"]
        for fact in task["required_facts"]
        if any(
            contains_asserted(normalized_answer, normalize(candidate))
            for candidate in fact["any_of"]
        )
    ]
    fact_score = len(facts_found) / len(task["required_facts"]) if task["required_facts"] else None
    penalty, forbidden_found = forbidden_penalty(answer, task["forbidden_claims"])
    raw = (
        address_score
        if task["type"] == "localization"
        else 0.5 * address_score + 0.5 * float(fact_score)
    )
    score = max(0.0, raw - penalty)
    transcript_path = run_dir / "transcript.jsonl"
    transcript = [
        json.loads(line)
        for line in transcript_path.read_text(encoding="utf-8").splitlines()
        if line
    ]
    navigation = None
    for record in transcript:
        if call_retrieves(record, arm, task["required_addresses"]):
            navigation = int(record["call_index"])
            break
    return {
        "task_id": task["id"],
        "arm": arm,
        "answer": answer.rstrip(),
        "address_score": address_score,
        "fact_score": fact_score,
        "penalty": penalty,
        "score": score,
        "required_addresses_found": required_found,
        "supporting_addresses_found": supporting_found,
        "facts_found": facts_found,
        "forbidden_claims_found": forbidden_found,
        "tool_calls": len(transcript),
        "tool_result_tokens": sum(int(r["result_tokens"]) for r in transcript),
        "navigation": navigation,
        "capped": (run_dir / "capped").exists(),
    }


def quartiles(values: list[float]) -> dict[str, float]:
    ordered = sorted(values)
    if len(ordered) < 2:
        return {"median": ordered[0], "q1": ordered[0], "q3": ordered[0], "iqr": 0.0}
    q1, _q2, q3 = statistics.quantiles(ordered, n=4, method="inclusive")
    return {
        "median": statistics.median(ordered),
        "q1": q1,
        "q3": q3,
        "iqr": q3 - q1,
    }


def main() -> None:
    task_bytes = (ROOT / "tasks.json").read_bytes()
    expected_blake3 = (ROOT / "gold.blake3").read_text().split()[0]
    expected_sha256 = (ROOT / "gold.sha256").read_text().split()[0]
    actual_blake3 = blake3(task_bytes).hexdigest()
    actual_sha256 = hashlib.sha256(task_bytes).hexdigest()
    if actual_blake3 != expected_blake3 or actual_sha256 != expected_sha256:
        raise SystemExit(f"gold hash mismatch: blake3={actual_blake3} sha256={actual_sha256}")
    runs: list[dict[str, Any]] = []
    for repetition in range(1, 4):
        for arm in ("agentlens", "baseline"):
            for task in TASKS:
                run_dir = RUNS_ROOT / f"repetition-{repetition}" / arm / task["id"]
                graded = grade_run(task, arm, run_dir)
                graded["repetition"] = repetition
                runs.append(graded)
    grouped: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for run in runs:
        grouped[(run["arm"], run["task_id"])].append(run)
    per_task: list[dict[str, Any]] = []
    for task in TASKS:
        row: dict[str, Any] = {"task_id": task["id"]}
        for arm in ("agentlens", "baseline"):
            arm_runs = grouped[(arm, task["id"])]
            row[arm] = {
                "score": quartiles([r["score"] for r in arm_runs]),
                "tool_result_tokens": quartiles([float(r["tool_result_tokens"]) for r in arm_runs]),
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
    for arm in ("agentlens", "baseline"):
        for repetition in range(1, 4):
            arm_runs = [
                run for run in runs if run["arm"] == arm and run["repetition"] == repetition
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
    arms: dict[str, Any] = {}
    for arm in ("agentlens", "baseline"):
        metrics = repetition_metrics[arm]
        arms[arm] = {
            "accuracy": quartiles([metric["accuracy"] for metric in metrics]),
            "cost_tokens_per_point": quartiles(
                [metric["cost_tokens_per_point"] for metric in metrics]
            ),
            "navigation": quartiles([metric["navigation_median"] for metric in metrics]),
            "repetitions": metrics,
            "capped_runs": sum(metric["capped_runs"] for metric in metrics),
        }
    head_to_head = {"agentlens_wins": 0, "baseline_wins": 0, "ties": 0}
    for row in per_task:
        left = row["agentlens"]["score"]["median"]
        right = row["baseline"]["score"]["median"]
        if left > right:
            head_to_head["agentlens_wins"] += 1
        elif left < right:
            head_to_head["baseline_wins"] += 1
        else:
            head_to_head["ties"] += 1
    cost_ratios = [
        repetition_metrics["agentlens"][index]["cost_tokens_per_point"]
        / repetition_metrics["baseline"][index]["cost_tokens_per_point"]
        for index in range(3)
    ]
    accuracy_deltas = [
        repetition_metrics["agentlens"][index]["accuracy"]
        - repetition_metrics["baseline"][index]["accuracy"]
        for index in range(3)
    ]
    cost_ratio = quartiles(cost_ratios)
    accuracy_delta = quartiles(accuracy_deltas)
    failures = {
        "cost_ratio_at_least_0_5": cost_ratio["median"] >= 0.5,
        "accuracy_more_than_0_05_below_baseline": accuracy_delta["median"] < -0.05,
        "loses_more_than_4_tasks": head_to_head["baseline_wins"] > 4,
    }
    result = {
        "subject": json.loads((ROOT / "protocol.json").read_text())["subject"],
        "gold_blake3": (ROOT / "gold.blake3").read_text().split()[0],
        "gold_sha256": (ROOT / "gold.sha256").read_text().split()[0],
        "arms": arms,
        "cost_ratio": cost_ratio,
        "accuracy_delta": accuracy_delta,
        "head_to_head": head_to_head,
        "falsification": failures,
        "benchmark_failed": any(failures.values()),
        "per_task": per_task,
        "runs": runs,
    }
    (ROOT / "results.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(
        json.dumps(
            {
                key: result[key]
                for key in (
                    "arms",
                    "cost_ratio",
                    "accuracy_delta",
                    "head_to_head",
                    "falsification",
                    "benchmark_failed",
                )
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
