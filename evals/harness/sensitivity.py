"""Bound one known bias in the localization rubric. Reporting aid, not the grader.

The worker prompt tells localization tasks to "return only the repo-relative
address(es) you would edit". *Address* is agentlens's own vocabulary: its output
is `path#Symbol`, so its answers carry symbol granularity for free. A control
arm reasonably reads the same word as "path" and answers `django/core/handlers/
base.py`, which the rubric scores zero because the gold names a symbol.

That asymmetry favours the tool under test, so the headline numbers must not be
the only ones published. This recomputes localization accuracy under a
deliberately control-friendly rule -- a required address counts as found if the
answer names its *file*, symbol or no -- and prints both. The pre-registered
rubric stays authoritative; this brackets how much the wording could be worth.
"""

from __future__ import annotations

import json
import statistics
from collections import defaultdict

import grade
from paths import ARMS, ROOT
from plan_runs import run_directory, scheduled_runs


def normalized_answer(run_id: str) -> str:
    answer = (run_directory(run_id) / "answer.txt").read_text(encoding="utf-8")
    return grade.normalize(answer.replace("\\", "/"))


def path_found(answer_normalized: str, gold: dict) -> bool:
    path = grade.normalize(gold["address"].split("#", 1)[0])
    return path in answer_normalized


def main() -> None:
    tasks = {task["id"]: task for task in json.loads((ROOT / "tasks.json").read_text())["tasks"]}
    strict: dict[str, list[float]] = defaultdict(list)
    lenient: dict[str, list[float]] = defaultdict(list)

    for run_id in scheduled_runs():
        task_id = run_id.rsplit("-", 1)[1]
        if not task_id.startswith("L"):
            continue
        arm = run_id.split("-", 2)[1]
        task = tasks[task_id]
        answer = (run_directory(run_id) / "answer.txt").read_text(encoding="utf-8")
        normalized = normalized_answer(run_id)
        required = task["required_addresses"]
        strict_hits = sum(grade.address_found(answer, gold) for gold in required)
        lenient_hits = sum(path_found(normalized, gold) for gold in required)
        strict[arm].append(strict_hits / len(required))
        lenient[arm].append(lenient_hits / len(required))

    print("localization address recall, 18 runs per arm (6 tasks x 3 repetitions)")
    for arm in ARMS:
        print(
            f"  {arm:10s} pre-registered {statistics.mean(strict[arm]):.3f}   "
            f"file-only {statistics.mean(lenient[arm]):.3f}"
        )

    print("\nper task, mean over 3 repetitions (pre-registered -> file-only)")
    for task_id in sorted(task for task in tasks if task.startswith("L")):
        cells = []
        for arm in ARMS:
            runs = [r for r in scheduled_runs() if r.endswith(task_id) and f"-{arm}-" in r]
            task = tasks[task_id]
            required = task["required_addresses"]
            s = statistics.mean(
                sum(
                    grade.address_found(
                        (run_directory(r) / "answer.txt").read_text(encoding="utf-8"), g
                    )
                    for g in required
                )
                / len(required)
                for r in runs
            )
            lenient_mean = statistics.mean(
                sum(path_found(normalized_answer(r), g) for g in required) / len(required)
                for r in runs
            )
            cells.append(f"{arm[:1].upper()} {s:.2f}->{lenient_mean:.2f}")
        print(f"  {task_id}  " + "   ".join(cells))

    headline_effect(tasks)


def headline_effect(tasks: dict) -> None:
    """What the campaign verdict becomes if the wording bias is removed entirely.

    Localization is 6 of 18 tasks, so a rubric artefact there moves the whole
    accuracy figure -- and cost is tokens per *point*, so it moves cost too.
    Recomputed from the same transcripts: only the localization address rule
    changes, and comprehension tasks keep their pre-registered scores.
    """
    points: dict[str, float] = defaultdict(float)
    tokens: dict[str, float] = defaultdict(float)
    scored: dict[str, int] = defaultdict(int)

    for run_id in scheduled_runs():
        arm = run_id.split("-", 2)[1]
        task_id = run_id.rsplit("-", 1)[1]
        task = tasks[task_id]
        directory = run_directory(run_id)
        answer = (directory / "answer.txt").read_text(encoding="utf-8")
        graded = grade.grade_run(task, arm, directory)
        if task_id.startswith("L"):
            required = task["required_addresses"]
            hits = sum(path_found(grade.normalize(answer.replace("\\", "/")), g) for g in required)
            penalty, _ = grade.forbidden_penalty(answer, task.get("forbidden_claims", []))
            score = max(0.0, hits / len(required) - penalty)
        else:
            score = float(graded["score"])
        points[arm] += score
        tokens[arm] += float(graded["tool_result_tokens"])
        scored[arm] += 1

    print("\nheadline under the file-only localization rule (comprehension unchanged)")
    cost = {arm: tokens[arm] / points[arm] for arm in ARMS}
    for arm in ARMS:
        print(
            f"  {arm:10s} accuracy {points[arm] / scored[arm]:.4f}   "
            f"cost {cost[arm]:8.0f} tokens/point"
        )
    for control in ("baseline", "linerange"):
        print(
            f"  agentlens vs {control:10s} cost ratio {cost['agentlens'] / cost[control]:.4f}   "
            f"accuracy delta {(points['agentlens'] - points[control]) / scored['agentlens']:+.4f}"
        )


if __name__ == "__main__":
    main()
