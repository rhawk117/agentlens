#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import subprocess
from collections import defaultdict

from blake3 import blake3

from attest import (
    EVENTS,
    MODEL,
    ROOT,
    RUNS_ROOT,
    expected_runs,
    load_tasks,
    parse_run_id,
    render_prompt,
)
from paths import AGENTLENS, DJANGO_ROOT, EXPECTED_RUNS, REPO_ROOT


def fail(message: str) -> None:
    raise SystemExit(message)


def result_fingerprint(record: dict) -> str:
    # ripgrep traverses files in parallel, so identical searches may emit the
    # same matches in a different order. Compare the complete result as a set of
    # output lines when the harness did not truncate it.
    if record["tool"] == "rg" and not record["truncated_by_harness"]:
        semantic_result = {
            "exit_code": record["exit_code"],
            "stdout_lines": sorted(record["stdout"].splitlines()),
            "stderr_lines": sorted(record["stderr"].splitlines()),
        }
        return hashlib.sha256(json.dumps(semantic_result, sort_keys=True).encode()).hexdigest()
    return record["full_result_sha256"]


def main() -> None:
    expected = expected_runs()
    events = [json.loads(line) for line in EVENTS.read_text(encoding="utf-8").splitlines() if line]
    starts = [event for event in events if event["event"] == "start"]
    completes = [event for event in events if event["event"] == "complete"]
    if len(starts) != EXPECTED_RUNS or len(completes) != EXPECTED_RUNS:
        fail(
            f"attestation event counts are not exactly {EXPECTED_RUNS} starts "
            f"and {EXPECTED_RUNS} completes"
        )
    if [event["run_id"] for event in starts] != expected:
        fail("start attestation order does not match schedule")
    if [event["sequence_index"] for event in starts] != list(range(1, EXPECTED_RUNS + 1)):
        fail("start sequence indexes are invalid")
    if len({event["run_id"] for event in completes}) != EXPECTED_RUNS or {
        event["run_id"] for event in completes
    } != set(expected):
        fail("completion attestations are incomplete")
    if len({event["agent_name"] for event in starts}) != EXPECTED_RUNS:
        fail("worker agent identities are not unique")
    if not all(
        event["fresh_session"] is True and event["fork_turns"] == "none" for event in starts
    ):
        fail("fresh-session attestation failed")
    if not all(event["model"] == MODEL for event in starts):
        fail("worker model attestation drift")
    start_by_run = {event["run_id"]: event for event in starts}
    complete_by_run = {event["run_id"]: event for event in completes}
    tasks = load_tasks()
    transcript_groups: dict[tuple[str, str, str, str], set[str]] = defaultdict(set)
    for run_id in expected:
        repetition, arm, task_id = parse_run_id(run_id)
        run_dir = RUNS_ROOT / f"repetition-{repetition}" / arm / task_id
        for name in ("prompt.txt", "metadata.json", "answer.txt", "transcript.jsonl"):
            if not (run_dir / name).exists():
                fail(f"{run_id}: missing {name}")
        prompt = (run_dir / "prompt.txt").read_text(encoding="utf-8")
        if prompt != render_prompt(run_id):
            fail(f"{run_id}: prompt rendering drift")
        if hashlib.sha256(prompt.encode()).hexdigest() != start_by_run[run_id]["prompt_sha256"]:
            fail(f"{run_id}: prompt hash mismatch")
        task_prompt = str(tasks[task_id]["prompt"])
        if (
            hashlib.sha256(task_prompt.encode()).hexdigest()
            != start_by_run[run_id]["task_prompt_sha256"]
        ):
            fail(f"{run_id}: task prompt hash mismatch")
        metadata = json.loads((run_dir / "metadata.json").read_text())
        if metadata != {
            "arm": arm,
            "repetition": repetition,
            "run_id": run_id,
            "task_id": task_id,
        }:
            fail(f"{run_id}: metadata mismatch")
        for artifact, event_key in (
            ("answer.txt", "answer_sha256"),
            ("transcript.jsonl", "transcript_sha256"),
        ):
            actual = hashlib.sha256((run_dir / artifact).read_bytes()).hexdigest()
            if actual != complete_by_run[run_id][event_key]:
                fail(f"{run_id}: completion hash mismatch for {artifact}")
        records = [
            json.loads(line)
            for line in (run_dir / "transcript.jsonl").read_text().splitlines()
            if line
        ]
        # Restated as literals rather than imported from bench_tool on purpose.
        # Importing would mean a cap loosened in the harness validates itself,
        # and the caps are part of the frozen v0.1.0 comparison.
        if len(records) > 25:
            fail(f"{run_id}: call cap exceeded")
        if sum(int(record["result_tokens"]) for record in records) > 60_000:
            fail(f"{run_id}: token cap exceeded")
        for record in records:
            key = (
                arm,
                task_id,
                record["tool"],
                json.dumps(record["args"], sort_keys=True),
            )
            transcript_groups[key].add(result_fingerprint(record))
    drift = {key: sorted(hashes) for key, hashes in transcript_groups.items() if len(hashes) > 1}
    if drift:
        fail(f"determinism drift for identical invocations: {drift}")
    task_bytes = (ROOT / "tasks.json").read_bytes()
    if blake3(task_bytes).hexdigest() != (ROOT / "gold.blake3").read_text().split()[0]:
        fail("gold BLAKE3 mismatch")
    protocol = json.loads((ROOT / "protocol.json").read_text())
    django_commit = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=DJANGO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if django_commit != protocol["subject"]["commit"]:
        fail("Django commit drift")
    # Not `HEAD == protocol.tool.commit`. Grading, this validator and the
    # results document are all committed after the binary is built, so that
    # equality fails on every commit that follows the campaign -- including the
    # one that publishes the numbers. What must hold is that the *tool* has not
    # moved: the pinned commit is still in this history, and nothing under
    # crates/ differs between it and HEAD. The binary hash below is the harder
    # guarantee; this catches a source change that was never rebuilt.
    pinned_tool_commit = protocol["tool"]["commit"]
    ancestry = subprocess.run(
        ["git", "merge-base", "--is-ancestor", pinned_tool_commit, "HEAD"],
        cwd=REPO_ROOT,
        capture_output=True,
    )
    if ancestry.returncode != 0:
        fail(f"pinned agentlens commit {pinned_tool_commit} is not an ancestor of HEAD")
    tool_diff = subprocess.run(
        ["git", "diff", "--quiet", pinned_tool_commit, "HEAD", "--", "crates", "Cargo.lock"],
        cwd=REPO_ROOT,
        capture_output=True,
    )
    if tool_diff.returncode != 0:
        fail("agentlens source changed since the pinned commit; rebuild and re-pin")
    binary_sha256 = hashlib.sha256(AGENTLENS.read_bytes()).hexdigest()
    if binary_sha256 != protocol["tool"]["binary_sha256"]:
        fail("agentlens binary drift")
    print(
        f"protocol validated: {EXPECTED_RUNS} fresh runs, "
        "schedule/order, caps, hashes, and determinism"
    )


if __name__ == "__main__":
    main()
