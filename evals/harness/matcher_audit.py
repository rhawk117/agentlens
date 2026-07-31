#!/usr/bin/env python3
"""Blind tuning and audit harness for the fact matcher.

The integrity problem this exists to solve: the same person wrote the tool
under test and is now editing the grader that scores it. A matcher tuned while
watching agentlens's score is indistinguishable from cheating, and nobody
reading the published number can tell the difference.

So the tuner never sees which arm produced an answer. `sample()` strips the arm
label before returning anything, and the corpus is split by *task* into a dev
half to tune against and a held-out half touched once, at the end. Per-arm
recall is computed only by `report()`, only after tuning is finished, and is
published whichever way it falls -- if v2 lifts agentlens more than the
controls, that asymmetry is a finding to disclose, not to hide.

    uv run python matcher_audit.py dev        # misses to tune against
    uv run python matcher_audit.py holdout    # recall on unseen tasks
    uv run python matcher_audit.py per-arm    # the disclosure, run last
"""

from __future__ import annotations

import json
import random
import sys
from dataclasses import dataclass

# Bound to the frozen v0.1.0 corpus, not to BENCH_RUNS_ROOT. Tuning is only
# ever done against answers that already exist; pointing this at a live
# campaign would mean tuning the grader on the runs it is about to grade.
from paths import ROOT
from paths import RUNS_ROOT_V1 as RUNS_ROOT

# Fixed so the dev/held-out split cannot be reshuffled until it flatters a
# result. Changing this value invalidates every recall number ever reported.
SPLIT_SEED = 424242
V1_REPETITIONS = (1, 2, 3)
V1_ARMS = ("agentlens", "baseline")


@dataclass(frozen=True)
class Sample:
    """One graded fact assertion, with the arm deliberately absent."""

    task_id: str
    fact_id: str
    phrases: tuple[str, ...]
    answer: str


def load_tasks() -> list[dict]:
    return json.loads((ROOT / "tasks.json").read_text(encoding="utf-8"))["tasks"]


def split_tasks() -> tuple[set[str], set[str]]:
    """Split task IDs into (dev, holdout), stratified by task type.

    Stratified because comprehension and localization tasks phrase facts
    differently; an unstratified split could put nearly all of one type in dev
    and leave the held-out score measuring something the tuner never saw.
    """
    tasks = load_tasks()
    dev: set[str] = set()
    holdout: set[str] = set()
    for kind in sorted({task["type"] for task in tasks}):
        ids = sorted(task["id"] for task in tasks if task["type"] == kind)
        random.Random(SPLIT_SEED).shuffle(ids)
        midpoint = len(ids) // 2
        dev.update(ids[:midpoint])
        holdout.update(ids[midpoint:])
    return dev, holdout


def sample(task_ids: set[str] | None = None) -> list[Sample]:
    """Every fact assertion from the frozen v0.1.0 answers, arm stripped."""
    samples: list[Sample] = []
    for task in load_tasks():
        if task_ids is not None and task["id"] not in task_ids:
            continue
        for repetition in V1_REPETITIONS:
            for arm in V1_ARMS:
                path = RUNS_ROOT / f"repetition-{repetition}" / arm / task["id"] / "answer.txt"
                if not path.exists():
                    continue
                answer = path.read_text(encoding="utf-8")
                for fact in task["required_facts"]:
                    samples.append(
                        Sample(
                            task_id=task["id"],
                            fact_id=fact["id"],
                            phrases=tuple(fact["any_of"]),
                            answer=answer,
                        )
                    )
    random.Random(SPLIT_SEED).shuffle(samples)
    return samples


def recall(samples: list[Sample], matcher) -> tuple[int, int]:
    matched = sum(
        1 for item in samples if any(matcher(item.answer, phrase) for phrase in item.phrases)
    )
    return matched, len(samples)


def main() -> None:
    from matching import phrase_matches as contains_asserted

    mode = sys.argv[1] if len(sys.argv) > 1 else "dev"
    dev_ids, holdout_ids = split_tasks()

    if mode == "dev":
        samples = sample(dev_ids)
        matched, total = recall(samples, contains_asserted)
        print(f"dev tasks: {sorted(dev_ids)}")
        print(f"dev recall: {matched}/{total} = {matched / total:.3f}\n")
        seen: set[tuple[str, str]] = set()
        for item in samples:
            if any(contains_asserted(item.answer, phrase) for phrase in item.phrases):
                continue
            key = (item.task_id, item.fact_id)
            if key in seen:
                continue
            seen.add(key)
            print(f"--- {item.task_id}/{item.fact_id} wants any of: {list(item.phrases)}")
            print(f"    answer: {' '.join(item.answer.split())[:300]}\n")
        return

    if mode == "holdout":
        matched, total = recall(sample(holdout_ids), contains_asserted)
        print(f"holdout tasks: {sorted(holdout_ids)}")
        print(f"HELD-OUT recall: {matched}/{total} = {matched / total:.3f}")
        return

    if mode == "per-arm":
        # The disclosure. Deliberately the only place arm labels are read.
        tasks = load_tasks()
        for arm in V1_ARMS:
            matched = total = 0
            for task in tasks:
                for repetition in V1_REPETITIONS:
                    path = RUNS_ROOT / f"repetition-{repetition}" / arm / task["id"] / "answer.txt"
                    if not path.exists():
                        continue
                    answer = path.read_text(encoding="utf-8")
                    for fact in task["required_facts"]:
                        total += 1
                        matched += any(contains_asserted(answer, p) for p in fact["any_of"])
            print(f"{arm:10s} {matched}/{total} = {matched / total:.3f}")
        return

    raise SystemExit("usage: matcher_audit.py <dev|holdout|per-arm>")


if __name__ == "__main__":
    main()
