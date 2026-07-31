#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import subprocess

from blake3 import blake3

from paths import AGENTLENS, DJANGO_ROOT, ROOT, TASK_COUNT

TASKS_PATH = ROOT / "tasks.json"


def fail(message: str) -> None:
    raise SystemExit(message)


def resolve(entry: dict[str, object]) -> None:
    address = str(entry["address"])
    expected_start = int(entry["start_line"])
    expected_end = int(entry["end_line"])
    completed = subprocess.run(
        [str(AGENTLENS), "slice", address, "--json", "--quiet"],
        cwd=DJANGO_ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        fail(f"{address}: did not resolve: {completed.stderr or completed.stdout}")
    result = json.loads(completed.stdout)
    matches = result.get("matches", [])
    if not matches:
        fail(f"{address}: resolved without a match")
    if not any(
        int(match["start_line"]) <= expected_start and int(match["end_line"]) >= expected_end
        for match in matches
    ):
        spans = [(match["start_line"], match["end_line"]) for match in matches]
        fail(f"{address}: expected {expected_start}-{expected_end}, got {spans}")


def main() -> None:
    task_bytes = TASKS_PATH.read_bytes()
    expected_sha256 = (ROOT / "gold.sha256").read_text().split()[0]
    expected_blake3 = (ROOT / "gold.blake3").read_text().split()[0]
    actual_sha256 = hashlib.sha256(task_bytes).hexdigest()
    actual_blake3 = blake3(task_bytes).hexdigest()
    if actual_sha256 != expected_sha256:
        fail(f"gold SHA-256 mismatch: {actual_sha256}")
    if actual_blake3 != expected_blake3:
        fail(f"gold BLAKE3 mismatch: {actual_blake3}")
    data = json.loads(TASKS_PATH.read_text())
    tasks = data["tasks"]
    ids = [task["id"] for task in tasks]
    if len(tasks) != TASK_COUNT or len(set(ids)) != TASK_COUNT:
        fail(f"expected {TASK_COUNT} unique tasks, got {len(tasks)} tasks and {len(set(ids))} IDs")
    if ids != [f"M{i:02d}" for i in range(1, 13)] + [f"L{i:02d}" for i in range(1, 7)]:
        fail(f"unexpected task order: {ids}")
    for task in tasks:
        if task["type"] == "comprehension" and not task["required_facts"]:
            fail(f"{task['id']}: comprehension task has no required facts")
        if task["type"] == "localization" and task["required_facts"]:
            fail(f"{task['id']}: localization task has required facts")
        for entry in task["required_addresses"] + task["supporting_addresses"]:
            resolve(entry)
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=DJANGO_ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if commit != data["subject"]["commit"]:
        fail(f"subject commit mismatch: {commit}")
    print(f"verified {len(tasks)} tasks and all gold addresses at {commit}")


if __name__ == "__main__":
    main()
