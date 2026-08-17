"""Campaign orchestration: dispatch in schedule order, resume from disk.

Ported semantics: plan_runs.py (a run is finished when answer.txt exists --
skip, never redo), attest.py (start events append in the pre-registered
schedule order; prompts and event hashes land on disk before dispatch), and
detect_leaks.py (every cited path must appear in the run's own transcript).

The schedule is read from the frozen harness schedule.json and never
regenerated: it stays pre-registered at 5 repetitions while campaigns execute
REPETITIONS = 3.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import re
from pathlib import Path

from agentlens_evals import subject, worker
from agentlens_evals.dataset import tasks_by_id
from agentlens_evals.paths import (
    ARMS,
    EXPECTED_RUNS,
    HARNESS_ROOT,
    REPETITIONS,
    RunId,
)

CITED_PATH = re.compile(r"\b((?:[\w.-]+/)+[\w.-]+\.py)\b")


class CampaignError(RuntimeError):
    pass


def scheduled_runs() -> list[str]:
    schedule = json.loads((HARNESS_ROOT / "schedule.json").read_text(encoding="utf-8"))[
        "schedule"
    ]
    runs: list[str] = []
    for repetition in schedule[:REPETITIONS]:
        for task_id in repetition["task_order"]:
            for arm in repetition["arm_order"]:
                runs.append(f"r{repetition['repetition']}-{arm}-{task_id}")
    return runs


def run_directory(run_id: str, runs_root: Path) -> Path:
    return RunId.parse(run_id).directory(runs_root)


def is_complete(run_id: str, runs_root: Path) -> bool:
    return (run_directory(run_id, runs_root) / "answer.txt").exists()


def campaign_status(runs_root: Path) -> dict:
    runs = scheduled_runs()
    complete = [run for run in runs if is_complete(run, runs_root)]
    outstanding = [run for run in runs if run not in set(complete)]
    stranded = [
        run
        for run in outstanding
        if (run_directory(run, runs_root) / "transcript.jsonl").exists()
    ]
    per_arm = {
        arm: sum(1 for run in complete if RunId.parse(run).arm == arm) for arm in ARMS
    }
    return {
        "complete": len(complete),
        "total": len(runs),
        "per_arm": per_arm,
        "stranded": stranded,
        "pending": len(outstanding),
    }


def events_path(runs_root: Path) -> Path:
    return runs_root / "attestations.jsonl"


def read_events(runs_root: Path) -> list[dict]:
    path = events_path(runs_root)
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def append_event(runs_root: Path, event: dict) -> None:
    runs_root.mkdir(parents=True, exist_ok=True)
    with events_path(runs_root).open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(event, sort_keys=True))
        stream.write("\n")


def record_start(runs_root: Path, run_id: str, prompt: str) -> None:
    started = [event for event in read_events(runs_root) if event["event"] == "start"]
    expected = [
        run
        for run in scheduled_runs()
        if run not in {event["run_id"] for event in started}
    ]
    if not expected or run_id != expected[0]:
        wanted = expected[0] if expected else "<none>"
        raise CampaignError(f"run order mismatch: expected {wanted}, got {run_id}")
    directory = run_directory(run_id, runs_root)
    directory.mkdir(parents=True, exist_ok=True)
    prompt_path = directory / "prompt.txt"
    if not prompt_path.exists():
        prompt_path.write_text(prompt, encoding="utf-8")
    append_event(
        runs_root,
        {
            "event": "start",
            "sequence_index": len(started) + 1,
            "run_id": run_id,
            "agent_name": f"sdk-worker-{run_id}",
            "model": worker.WORKER_MODEL,
            "fresh_session": True,
            "fork_turns": "none",
            "prompt_sha256": hashlib.sha256(prompt.encode()).hexdigest(),
        },
    )


def record_complete(runs_root: Path, run_id: str) -> None:
    directory = run_directory(run_id, runs_root)
    answer = directory / "answer.txt"
    transcript = directory / "transcript.jsonl"
    if not answer.exists() or not transcript.exists():
        raise CampaignError(f"incomplete artifacts for {run_id}")
    append_event(
        runs_root,
        {
            "event": "complete",
            "run_id": run_id,
            "answer_sha256": hashlib.sha256(answer.read_bytes()).hexdigest(),
            "transcript_sha256": hashlib.sha256(transcript.read_bytes()).hexdigest(),
        },
    )


def transcript_text(run_id: str, runs_root: Path) -> str | None:
    path = run_directory(run_id, runs_root) / "transcript.jsonl"
    if not path.exists():
        return None
    chunks: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line:
            continue
        record = json.loads(line)
        chunks.append(record.get("stdout", ""))
        chunks.append(record.get("stderr", ""))
        chunks.append(" ".join(str(arg) for arg in record.get("args", [])))
    return "\n".join(chunks)


def unsupported_citations(run_id: str, runs_root: Path) -> list[str] | None:
    answer_path = run_directory(run_id, runs_root) / "answer.txt"
    if not answer_path.exists():
        return None
    transcript = transcript_text(run_id, runs_root)
    if transcript is None:
        return None
    answer = answer_path.read_text(encoding="utf-8")
    cited = {match.group(1) for match in CITED_PATH.finditer(answer.replace("\\", "/"))}
    return sorted(path for path in cited if path not in transcript)


def classify(run_id: str, runs_root: Path) -> str | None:
    leaks = unsupported_citations(run_id, runs_root)
    if leaks is None:
        return None
    if not leaks:
        return "clean"
    capped = (run_directory(run_id, runs_root) / "capped").exists()
    return "memorisation" if capped else "breach"


async def run_campaign(version: str, runs_root: Path, concurrency: int = 4) -> dict:
    binary = subject.verify(version)
    runs = scheduled_runs()
    if len(runs) != EXPECTED_RUNS:
        raise CampaignError(
            f"schedule yields {len(runs)} runs, expected {EXPECTED_RUNS}"
        )
    tasks = tasks_by_id()
    outstanding = [run for run in runs if not is_complete(run, runs_root)]

    started = {
        event["run_id"] for event in read_events(runs_root) if event["event"] == "start"
    }
    semaphore = asyncio.Semaphore(concurrency)

    async def execute(run_id: str) -> bool:
        task_id = RunId.parse(run_id).task_id
        prompt = worker.render_prompt(run_id, tasks[task_id].prompt)
        options = worker.worker_options(run_id, runs_root, binary)
        async with semaphore:
            try:
                await worker.dispatch(run_id, prompt, options)
            except worker.DispatchError:
                return False
        return True

    pending: list[asyncio.Task] = []
    for run_id in outstanding:
        if run_id not in started:
            task_id = RunId.parse(run_id).task_id
            record_start(
                runs_root, run_id, worker.render_prompt(run_id, tasks[task_id].prompt)
            )
        pending.append(asyncio.ensure_future(execute(run_id)))
    dispatch_results = await asyncio.gather(*pending) if pending else []
    failed_dispatches = sum(1 for succeeded in dispatch_results if not succeeded)

    completed_events = {
        event["run_id"]
        for event in read_events(runs_root)
        if event["event"] == "complete"
    }
    for run_id in runs:
        if is_complete(run_id, runs_root) and run_id not in completed_events:
            record_complete(runs_root, run_id)

    breaches = [run for run in runs if classify(run, runs_root) == "breach"]
    memorised = [run for run in runs if classify(run, runs_root) == "memorisation"]
    return {
        "dispatched": len(outstanding),
        "complete": sum(is_complete(run, runs_root) for run in runs),
        "breaches": breaches,
        "memorised_capped": memorised,
        "failed_dispatches": failed_dispatches,
    }
