#!/usr/bin/env python3
"""Generate the run schedule deterministically from recorded seeds.

The v0.1.0 schedule was a hand-written literal, so its "shuffled" task orders
could not be checked against anything -- a reader had to take on faith that
they were not chosen to favour an arm. Here the seed is the input and the file
is the output, so anyone can rerun this and diff.

Two orderings are randomised, for two different biases:

* Task order, per repetition. A worker's later tasks in a session are not
  independent of its earlier ones; fixing the order would bake that in.
* Arm order, rotated rather than shuffled. With 3 arms over 5 repetitions a
  rotation guarantees each arm leads at least once, which a shuffle does not.
"""

from __future__ import annotations

import json
import random

from paths import ARMS, REPETITIONS, ROOT

# Recorded, not derived: the campaign is reproducible only if these are fixed.
BASE_SEED = 20260730
TASK_IDS = [f"M{index:02d}" for index in range(1, 13)] + [f"L{index:02d}" for index in range(1, 7)]


def schedule_for(repetition: int) -> dict[str, object]:
    seed = BASE_SEED + repetition
    task_order = list(TASK_IDS)
    random.Random(seed).shuffle(task_order)
    offset = (repetition - 1) % len(ARMS)
    return {
        "repetition": repetition,
        "seed": seed,
        "task_order": task_order,
        "arm_order": list(ARMS[offset:] + ARMS[:offset]),
    }


def main() -> None:
    schedule = {
        "generator": "make_schedule.py",
        "base_seed": BASE_SEED,
        "schedule": [schedule_for(index) for index in range(1, REPETITIONS + 1)],
    }
    path = ROOT / "schedule.json"
    path.write_text(json.dumps(schedule, indent=2) + "\n", encoding="utf-8")
    runs = REPETITIONS * len(TASK_IDS) * len(ARMS)
    print(
        f"wrote {path.name}: {REPETITIONS} repetitions"
        f" x {len(TASK_IDS)} tasks x {len(ARMS)} arms = {runs} runs"
    )


if __name__ == "__main__":
    main()
