#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent
RUNS_ROOT = ROOT / "runs"
EVENTS = ROOT / "attestations.jsonl"
BENCH_PY = ROOT / ".venv" / "bin" / "python"
BENCH_TOOL = ROOT / "bench_tool.py"
MODEL = "gpt-5.6-sol (inherited worker model)"


def load_tasks() -> dict[str, dict[str, object]]:
    data = json.loads((ROOT / "tasks.json").read_text(encoding="utf-8"))
    return {task["id"]: task for task in data["tasks"]}


def expected_runs() -> list[str]:
    schedule = json.loads((ROOT / "schedule.json").read_text())["schedule"]
    runs: list[str] = []
    for repetition in schedule:
        for task_id in repetition["task_order"]:
            for arm in repetition["arm_order"]:
                runs.append(f"r{repetition['repetition']}-{arm}-{task_id}")
    return runs


def parse_run_id(run_id: str) -> tuple[int, str, str]:
    repetition, arm, task_id = run_id.split("-", 2)
    return int(repetition[1:]), arm, task_id


def render_prompt(run_id: str) -> str:
    _repetition, arm, task_id = parse_run_id(run_id)
    task = load_tasks()[task_id]
    common = (
        "You are one independent benchmark worker answering one question about the "
        "pinned Django 6.0.7 source checkout. Use only evidence retrieved during "
        "this run. Do not inspect the rubric, gold files, benchmark implementation, "
        "or another run.\n\n"
        "Do not use direct file reads, shell search commands, web access, prior "
        "knowledge, or any other tool to inspect the subject. You may invoke only "
        "the benchmark wrapper through the shell. Stop researching when it reports "
        "a cap.\n\n"
        "For comprehension tasks, answer concisely with the behavior and "
        "repo-relative source address(es). For localization tasks, return only the "
        "repo-relative address(es) you would edit.\n\n"
    )
    if arm == "agentlens":
        interface = (
            "Available research commands: slice, map, find, literals, callers, "
            "packet, and dead.\n"
        )
    else:
        interface = (
            "Available research commands: rg matches and cat whole-file reads. "
            "Line-range and context reads are unavailable.\n"
        )
    command = (
        f"\nInvoke research as:\n{BENCH_PY} {BENCH_TOOL} {run_id} COMMAND ARGS...\n\n"
        "After composing the answer, submit exactly once as:\n"
        f"{BENCH_PY} {BENCH_TOOL} {run_id} submit 'COMPLETE ANSWER'\n\n"
        "Then return only `submitted` as your final response.\n\n"
        f"Task {task_id}: {task['prompt']}\n"
    )
    return common + interface + command


def read_events() -> list[dict[str, object]]:
    if not EVENTS.exists():
        return []
    return [json.loads(line) for line in EVENTS.read_text().splitlines() if line]


def append_event(event: dict[str, object]) -> None:
    with EVENTS.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(event, sort_keys=True))
        stream.write("\n")


def start(pairs: list[str]) -> None:
    if len(pairs) % 2:
        raise SystemExit("start expects RUN_ID AGENT_NAME pairs")
    events = read_events()
    started = [event for event in events if event["event"] == "start"]
    expected = expected_runs()
    for offset in range(0, len(pairs), 2):
        run_id, agent_name = pairs[offset : offset + 2]
        index = len(started)
        if index >= len(expected) or run_id != expected[index]:
            wanted = expected[index] if index < len(expected) else "<none>"
            raise SystemExit(f"run order mismatch: expected {wanted}, got {run_id}")
        prompt = render_prompt(run_id)
        repetition, arm, task_id = parse_run_id(run_id)
        run_dir = RUNS_ROOT / f"repetition-{repetition}" / arm / task_id
        run_dir.mkdir(parents=True, exist_ok=True)
        prompt_path = run_dir / "prompt.txt"
        if prompt_path.exists():
            raise SystemExit(f"prompt already exists for {run_id}")
        prompt_path.write_text(prompt, encoding="utf-8")
        event = {
            "event": "start",
            "sequence_index": index + 1,
            "run_id": run_id,
            "agent_name": agent_name,
            "model": MODEL,
            "fresh_session": True,
            "fork_turns": "none",
            "prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
            "task_prompt_sha256": hashlib.sha256(
                str(load_tasks()[task_id]["prompt"]).encode()
            ).hexdigest(),
        }
        append_event(event)
        started.append(event)
        print(prompt_path)


def complete(run_ids: list[str]) -> None:
    events = read_events()
    starts = {event["run_id"] for event in events if event["event"] == "start"}
    completed = {event["run_id"] for event in events if event["event"] == "complete"}
    for run_id in run_ids:
        if run_id not in starts or run_id in completed:
            raise SystemExit(f"cannot complete {run_id}")
        repetition, arm, task_id = parse_run_id(run_id)
        run_dir = RUNS_ROOT / f"repetition-{repetition}" / arm / task_id
        answer = run_dir / "answer.txt"
        transcript = run_dir / "transcript.jsonl"
        if not answer.exists() or not transcript.exists():
            raise SystemExit(f"incomplete artifacts for {run_id}")
        append_event(
            {
                "event": "complete",
                "run_id": run_id,
                "answer_sha256": hashlib.sha256(answer.read_bytes()).hexdigest(),
                "transcript_sha256": hashlib.sha256(transcript.read_bytes()).hexdigest(),
            }
        )
        completed.add(run_id)


def main() -> None:
    if len(sys.argv) < 3:
        raise SystemExit("usage: attest.py <start|complete|prompt> ARGS...")
    command, *args = sys.argv[1:]
    if command == "start":
        start(args)
    elif command == "complete":
        complete(args)
    elif command == "prompt" and len(args) == 1:
        print(render_prompt(args[0]), end="")
    else:
        raise SystemExit("invalid attest command")


if __name__ == "__main__":
    main()
