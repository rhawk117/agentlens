"""Metering wrapper subprocess tests.

test_linerange_admits_exactly_one_sed_form's malformed forms each probe a
distinct sed attack: a command riding the print range, a missing file
argument, an in-place edit, and a shell escape.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path

import pytest

RUN_ID = "r1-baseline-M01"


@pytest.fixture()
def corpus(tmp_path: Path) -> Path:
    root = tmp_path / "corpus"
    (root / "pkg").mkdir(parents=True)
    (root / "pkg" / "mod.py").write_text(
        "\n".join(f"line {n}" for n in range(1, 31)) + "\n", encoding="utf-8"
    )
    return root


@pytest.fixture()
def runs_root(tmp_path: Path) -> Path:
    root = tmp_path / "runs"
    root.mkdir()
    return root


def invoke(corpus: Path, runs_root: Path, *argv: str, agentlens: str | None = None):
    env = {k: v for k, v in os.environ.items() if not k.startswith("BENCH_")}
    env["BENCH_DJANGO_ROOT"] = str(corpus)
    env["BENCH_RUNS_ROOT"] = str(runs_root)
    if agentlens:
        env["BENCH_AGENTLENS"] = agentlens
    return subprocess.run(
        [sys.executable, "-m", "agentlens_evals.metering", *argv],
        capture_output=True,
        text=True,
        env=env,
    )


def run_dir(runs_root: Path, run_id: str = RUN_ID) -> Path:
    repetition, arm, task = run_id.split("-", 2)
    return runs_root / f"repetition-{repetition[1:]}" / arm / task


def test_baseline_cat_is_metered(corpus, runs_root) -> None:
    result = invoke(corpus, runs_root, RUN_ID, "cat", "pkg/mod.py")
    assert result.returncode == 0
    assert "line 30" in result.stdout
    records = [
        json.loads(line)
        for line in (run_dir(runs_root) / "transcript.jsonl").read_text().splitlines()
    ]
    assert len(records) == 1 and records[0]["result_tokens"] > 0


def test_baseline_refuses_sed(corpus, runs_root) -> None:
    result = invoke(corpus, runs_root, RUN_ID, "sed", "-n", "1,5p", "pkg/mod.py")
    assert result.returncode == 2
    assert "unavailable in the baseline arm" in result.stderr


def test_linerange_admits_exactly_one_sed_form(corpus, runs_root) -> None:
    ok = invoke(
        corpus, runs_root, "r1-linerange-M01", "sed", "-n", "1,5p", "pkg/mod.py"
    )
    assert ok.returncode == 0 and "line 5" in ok.stdout
    for args in (
        ["-n", "1,5p;w /tmp/x", "pkg/mod.py"],
        ["-n", "1,5p"],
        ["-i", "1,5p", "pkg/mod.py"],
        ["-n", "e id", "pkg/mod.py"],
    ):
        result = invoke(corpus, runs_root, "r1-linerange-M01", "sed", *args)
        assert result.returncode == 2, args


def test_traversal_and_absolute_paths_refused(corpus, runs_root) -> None:
    for arg in ("../outside.py", "/etc/passwd"):
        result = invoke(corpus, runs_root, RUN_ID, "cat", arg)
        assert result.returncode == 2, arg


def test_submit_once(corpus, runs_root) -> None:
    first = invoke(corpus, runs_root, RUN_ID, "submit", "the answer")
    assert first.returncode == 0
    assert (run_dir(runs_root) / "answer.txt").read_text() == "the answer\n"
    second = invoke(corpus, runs_root, RUN_ID, "submit", "revised")
    assert second.returncode == 2
    assert "already submitted" in second.stderr


def test_call_cap_marks_and_refuses(corpus, runs_root) -> None:
    directory = run_dir(runs_root)
    directory.mkdir(parents=True)
    with (directory / "transcript.jsonl").open("w") as stream:
        for index in range(25):
            stream.write(
                json.dumps({"call_index": index + 1, "result_tokens": 1}) + "\n"
            )
    result = invoke(corpus, runs_root, RUN_ID, "cat", "pkg/mod.py")
    assert result.returncode == 3
    assert (directory / "capped").exists()


def test_token_cap_marks_and_refuses(corpus, runs_root) -> None:
    directory = run_dir(runs_root)
    directory.mkdir(parents=True)
    (directory / "transcript.jsonl").write_text(
        json.dumps({"call_index": 1, "result_tokens": 60_000}) + "\n"
    )
    result = invoke(corpus, runs_root, RUN_ID, "cat", "pkg/mod.py")
    assert result.returncode == 3
    assert (directory / "capped").exists()


def test_agentlens_arm_requires_pinned_binary(corpus, runs_root) -> None:
    result = invoke(corpus, runs_root, "r1-agentlens-M01", "map", ".")
    assert result.returncode == 2
    assert "BENCH_AGENTLENS is not set" in result.stderr
