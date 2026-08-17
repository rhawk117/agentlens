"""The frozen task set as typed models and a pydantic-evals Dataset.

tasks.json is the pre-registered gold: 18 tasks, comprehension and
localization, each with gold addresses, accepted fact phrasings, and forbidden
claims. Its hashes are pre-registered beside it; loading verifies both, the
same check grade.py made, so a graded number can never come from an edited
task set.
"""

from __future__ import annotations

import hashlib
import json
from typing import Literal

from blake3 import blake3
from pydantic import BaseModel, ConfigDict
from pydantic_evals import Case, Dataset

from agentlens_evals.paths import HARNESS_ROOT


class GoldError(RuntimeError):
    """The task set does not match its pre-registered hashes."""


class GoldAddress(BaseModel):
    model_config = ConfigDict(extra="ignore")
    address: str
    start_line: int
    end_line: int


class Fact(BaseModel):
    model_config = ConfigDict(extra="ignore")
    id: str
    any_of: list[str]


class BenchTask(BaseModel):
    model_config = ConfigDict(extra="ignore")
    id: str
    type: Literal["comprehension", "localization"]
    prompt: str
    required_addresses: list[GoldAddress]
    supporting_addresses: list[GoldAddress]
    required_facts: list[Fact]
    forbidden_claims: list[str]


def verify_gold() -> bytes:
    # HARNESS_ROOT is read as a module global so tests can point it elsewhere.
    task_bytes = (HARNESS_ROOT / "tasks.json").read_bytes()
    expected_blake3 = (HARNESS_ROOT / "gold.blake3").read_text().split()[0]
    expected_sha256 = (HARNESS_ROOT / "gold.sha256").read_text().split()[0]
    actual_blake3 = blake3(task_bytes).hexdigest()
    actual_sha256 = hashlib.sha256(task_bytes).hexdigest()
    if actual_blake3 != expected_blake3 or actual_sha256 != expected_sha256:
        raise GoldError(
            f"gold hash mismatch: blake3={actual_blake3} sha256={actual_sha256}"
        )
    return task_bytes


def load_tasks() -> list[BenchTask]:
    data = json.loads(verify_gold().decode("utf-8"))
    return [BenchTask.model_validate(task) for task in data["tasks"]]


def tasks_by_id() -> dict[str, BenchTask]:
    return {task.id: task for task in load_tasks()}


def build_dataset(
    tasks: list[BenchTask],
    arm: str,
    repetition: int,
    task_order: list[str] | None = None,
) -> Dataset:
    by_id = {task.id: task for task in tasks}
    order = task_order or [task.id for task in tasks]
    cases = [
        Case(
            name=task_id,
            inputs=f"r{repetition}-{arm}-{task_id}",
            metadata=by_id[task_id].model_dump(),
        )
        for task_id in order
    ]
    return Dataset(name=f"r{repetition}-{arm}", cases=cases)
