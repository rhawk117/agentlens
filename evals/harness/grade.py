#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

from blake3 import blake3

from matching import fact_satisfied
from paths import ARMS, DJANGO_ROOT, REPETITIONS, ROOT, RUNS_ROOT

TASKS = json.loads((ROOT / "tasks.json").read_text(encoding="utf-8"))["tasks"]
# Recorded in every results file. A score is only comparable to another score
# graded by the same matcher, and v1 and v2 differ by more than 60 points of
# recall -- quoting one against the other would be meaningless.
MATCHER_VERSION = 2
# The arm under test. Every other arm is a control it is compared against.
REFERENCE_ARM = "agentlens"
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
    if tool == "sed":
        # Arm C reads a span rather than a file, so retrieval means the span it
        # asked for overlaps the gold span -- not merely that it named the file.
        # Without this, every Arm C run would score as never having found
        # anything and its navigation metric would be uniformly capped.
        if int(record["exit_code"]) != 0 or not record["stdout"] or len(args) != 3:
            return False
        span = re.fullmatch(r"(\d+)(?:,(\d+))?p", args[1])
        if not span:
            return False
        first = int(span.group(1))
        last = int(span.group(2) or first)
        requested_path = args[2].replace("\\", "/")
        return any(
            gold["address"].split("#", 1)[0] == requested_path
            and first <= int(gold["end_line"])
            and last >= int(gold["start_line"])
            for gold in golds
        )
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
    facts_found = [fact["id"] for fact in task["required_facts"] if fact_satisfied(answer, fact)]
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
    parser = argparse.ArgumentParser(description="Grade a benchmark campaign.")
    parser.add_argument(
        "--repetitions",
        type=int,
        default=REPETITIONS,
        help="how many repetitions to grade (v0.1.0 ran 3, v0.2.0 runs 5)",
    )
    parser.add_argument(
        "--arms",
        nargs="+",
        default=list(ARMS),
        help="arms to grade; the v0.1.0 corpus has only agentlens and baseline",
    )
    parser.add_argument("--output", default="results.json")
    options = parser.parse_args()
    repetitions = range(1, options.repetitions + 1)
    arms_to_grade = list(options.arms)
    if REFERENCE_ARM not in arms_to_grade:
        raise SystemExit(f"{REFERENCE_ARM} must be among the graded arms")

    task_bytes = (ROOT / "tasks.json").read_bytes()
    expected_blake3 = (ROOT / "gold.blake3").read_text().split()[0]
    expected_sha256 = (ROOT / "gold.sha256").read_text().split()[0]
    actual_blake3 = blake3(task_bytes).hexdigest()
    actual_sha256 = hashlib.sha256(task_bytes).hexdigest()
    if actual_blake3 != expected_blake3 or actual_sha256 != expected_sha256:
        raise SystemExit(f"gold hash mismatch: blake3={actual_blake3} sha256={actual_sha256}")
    runs: list[dict[str, Any]] = []
    for repetition in repetitions:
        for arm in arms_to_grade:
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
        for arm in arms_to_grade:
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
    for arm in arms_to_grade:
        for repetition in repetitions:
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
    for arm in arms_to_grade:
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
    # agentlens is compared against each control separately. Pooling the
    # controls would let a weak arm flatter the tool: the whole reason Arm C
    # exists is that beating Arm B is a lower bar than beating a competent
    # operator, and an average across the two would hide exactly that.
    protocol = json.loads((ROOT / "protocol.json").read_text())
    thresholds = protocol["falsification"]
    comparisons: dict[str, Any] = {}
    for control in [arm for arm in arms_to_grade if arm != REFERENCE_ARM]:
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
        indices = range(len(repetitions))
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
            "too_many_tasks_lost": (head_to_head["control_wins"] > limits["maximum_tasks_lost"]),
        }
        comparisons[control] = {
            "cost_ratio": cost_ratio,
            "accuracy_delta": accuracy_delta,
            "head_to_head": head_to_head,
            "thresholds": limits,
            "falsification": failures,
            "failed": any(failures.values()),
        }
    result = {
        "subject": protocol["subject"],
        "matcher_version": MATCHER_VERSION,
        "repetitions_graded": list(repetitions),
        "arms_graded": list(arms_to_grade),
        "gold_blake3": (ROOT / "gold.blake3").read_text().split()[0],
        "gold_sha256": (ROOT / "gold.sha256").read_text().split()[0],
        "arms": arms,
        "comparisons": comparisons,
        "benchmark_failed": any(entry["failed"] for entry in comparisons.values()),
        "per_task": per_task,
        "runs": runs,
    }
    (ROOT / options.output).write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    summary = {
        "matcher_version": MATCHER_VERSION,
        "arms": {arm: arms[arm]["accuracy"]["median"] for arm in arms_to_grade},
        "comparisons": {
            control: {
                "cost_ratio": entry["cost_ratio"]["median"],
                "accuracy_delta": entry["accuracy_delta"]["median"],
                "head_to_head": entry["head_to_head"],
                "failed": entry["failed"],
            }
            for control, entry in comparisons.items()
        },
        "benchmark_failed": result["benchmark_failed"],
    }
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
