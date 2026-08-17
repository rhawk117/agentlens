"""Verification gate 1: the port must reproduce grade.py per-run scores exactly.

Loads the frozen harness grade.py by file path and compares its grade_run
output against the ported grading.grade_run for all 162 runs_v2 runs. Any
divergence is a port bug until proven otherwise.
"""

from __future__ import annotations

import importlib.util
import sys

import pytest

from agentlens_evals import grading, paths

RUNS_V2 = paths.REPO_ROOT / ".eval" / "runs_v2"


def _missing_corpus_reason() -> str | None:
    missing = []
    if not RUNS_V2.is_dir():
        missing.append(f"the runs_v2 corpus at {RUNS_V2}")
    if not paths.django_root().is_dir():
        missing.append(f"the Django checkout at {paths.django_root()}")
    if not missing:
        return None
    return (
        f"missing {' and '.join(missing)}: the parity gate cannot compare "
        "grading.grade_run against the frozen harness without the archived corpus"
    )


_CORPUS_SKIP_REASON = _missing_corpus_reason()
pytestmark = pytest.mark.skipif(
    _CORPUS_SKIP_REASON is not None,
    reason=_CORPUS_SKIP_REASON or "",
)


def load_harness_module(name: str):
    if str(paths.HARNESS_ROOT) not in sys.path:
        sys.path.insert(0, str(paths.HARNESS_ROOT))
    spec = importlib.util.spec_from_file_location(
        name, paths.HARNESS_ROOT / f"{name}.py"
    )
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_every_runs_v2_grade_is_reproduced() -> None:
    harness_grade = load_harness_module("grade")
    tasks = {task["id"]: task for task in harness_grade.TASKS}
    compared = 0
    for repetition in range(1, paths.REPETITIONS + 1):
        for arm in paths.ARMS:
            for task_id, task in tasks.items():
                run_dir = RUNS_V2 / f"repetition-{repetition}" / arm / task_id
                if not (run_dir / "answer.txt").exists():
                    pytest.fail(f"runs_v2 is missing {repetition}/{arm}/{task_id}")
                expected = harness_grade.grade_run(task, arm, run_dir)
                actual = grading.grade_run(task, arm, run_dir)
                assert actual == expected, (
                    f"divergence at r{repetition}-{arm}-{task_id}"
                )
                compared += 1
    assert compared == paths.EXPECTED_RUNS
