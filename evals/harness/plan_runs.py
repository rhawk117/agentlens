#!/usr/bin/env python3
"""Emit the run IDs still to be executed, in schedule order.

The campaign is 270 sessions. Something will fail partway -- a rate limit, a
worker that never submits, an interrupted batch -- so the driver has to be
resumable, and resumability has to mean *skip*, not *redo*: `submit` refuses to
overwrite an existing answer, so re-dispatching a finished run just burns a
session and leaves a confusing dead transcript behind.

A run is finished when its answer.txt exists. Anything else -- a directory with
a transcript but no answer, a capped marker with no answer -- is unfinished and
comes back in the list.

    uv run python plan_runs.py                 # everything outstanding
    uv run python plan_runs.py --limit 12      # the next batch
    uv run python plan_runs.py --repetition 1  # one repetition
    uv run python plan_runs.py --status        # counts, not IDs
"""

from __future__ import annotations

import argparse
import json

from paths import ARMS, EXPECTED_RUNS, REPETITIONS, ROOT, RUNS_ROOT


def scheduled_runs() -> list[str]:
    """Every run ID of the executed campaign, in the order the schedule gives.

    schedule.json still carries the pre-registered repetitions 4 and 5. They
    were never executed, so they are truncated here rather than deleted there:
    the file stays as pre-registered, and every consumer -- the driver, the
    leak detector, the protocol validator -- agrees on the same boundary
    because they all read this one function.
    """
    schedule = json.loads((ROOT / "schedule.json").read_text(encoding="utf-8"))["schedule"]
    runs: list[str] = []
    for repetition in schedule[:REPETITIONS]:
        for task_id in repetition["task_order"]:
            for arm in repetition["arm_order"]:
                runs.append(f"r{repetition['repetition']}-{arm}-{task_id}")
    return runs


def run_directory(run_id: str):
    repetition, arm, task_id = run_id.split("-", 2)
    return RUNS_ROOT / f"repetition-{repetition[1:]}" / arm / task_id


def is_complete(run_id: str) -> bool:
    return (run_directory(run_id) / "answer.txt").exists()


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--limit", type=int, help="emit at most this many run IDs")
    parser.add_argument("--repetition", type=int, help="restrict to one repetition")
    parser.add_argument("--arm", choices=ARMS, help="restrict to one arm")
    parser.add_argument("--status", action="store_true", help="summarise instead of listing")
    options = parser.parse_args()

    runs = scheduled_runs()
    if len(runs) != EXPECTED_RUNS:
        raise SystemExit(f"schedule yields {len(runs)} runs, protocol expects {EXPECTED_RUNS}")
    if options.repetition is not None:
        runs = [run for run in runs if run.startswith(f"r{options.repetition}-")]
    if options.arm:
        runs = [run for run in runs if run.split("-", 2)[1] == options.arm]

    outstanding = [run for run in runs if not is_complete(run)]

    if options.status:
        done = len(runs) - len(outstanding)
        print(f"complete   {done}/{len(runs)}")
        for arm in ARMS:
            in_arm = [run for run in runs if run.split("-", 2)[1] == arm]
            finished = sum(1 for run in in_arm if is_complete(run))
            print(f"  {arm:10s} {finished}/{len(in_arm)}")
        # Started but never finished: these are the runs that need a re-dispatch,
        # and they are invisible in a plain completion count.
        stranded = [
            run for run in outstanding if (run_directory(run) / "transcript.jsonl").exists()
        ]
        if stranded:
            print(f"stranded (transcript, no answer): {len(stranded)}")
            for run in stranded:
                print(f"  {run}")
        return

    for run in outstanding[: options.limit]:
        print(run)


if __name__ == "__main__":
    main()
