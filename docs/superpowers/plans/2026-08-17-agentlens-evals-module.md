# agentlens-evals Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A uv package `evals/agentlens_evals/` with console script `agentlens-evals` that installs a hash-pinned agentlens binary, runs a full benchmark campaign through claude-agent-sdk workers, grades it with ported (bit-identical) matcher-v2 logic exposed as pydantic-evals evaluators, and reports against the pre-registered falsification thresholds.

**Architecture:** The existing `evals/harness/` stays frozen as the v1/v2 record and as the source of the campaign inputs (`tasks.json`, `gold.*`, `protocol.json`, `schedule.json`) — the new module reads those files and never regenerates them. Grading logic is ported function-for-function from `grade.py`/`matching.py` so a re-grade of the completed `runs_v2` corpus reproduces every per-run score exactly. Execution (dispatch) is driven by the module in schedule order with bounded concurrency, because the sequential start-order attestation is kept (spec default); pydantic-evals owns the *evaluation* passes — the task callable loads a completed run from disk, evaluators score it, and its reports are derived artifacts. Disk (`answer.txt` / `transcript.jsonl` / `capped`) remains the source of truth for resumability.

**Tech Stack:** Python ≥3.12, uv, pydantic ≥2, pydantic-evals 2.31.x, claude-agent-sdk, tiktoken ==0.8.0 (pinned — token counts are the headline metric), blake3, pytest + pytest-asyncio.

## Global Constraints

- Worker model is `claude-haiku-4-5-20251001`, pinned as the code constant `WORKER_MODEL`. Dispatch must fail loudly on any other model (verification gate 4).
- Tokenizer pinned: tiktoken `o200k_base`, package `tiktoken==0.8.0`. Never change either.
- Matcher version 2. Grading logic is a verbatim port; any per-run divergence from `evals/harness/grade.py` on the runs_v2 corpus is a port bug (verification gate 1).
- `evals/bin/` is gitignored. `target/release/` is never consulted at campaign time; the metering gate takes the binary path only from `BENCH_AGENTLENS`, which the campaign sets from a hash-verified manifest (verification gate 3).
- The module must never write to `evals/harness/` and never regenerate `schedule.json` (stays at 5 pre-registered repetitions; campaigns execute 3) or `protocol.json`.
- Subject corpus: `~/dev/django-6.0.7` (override `BENCH_DJANGO_ROOT`), commit `e2a424605ac2e7e6e799496542fb2997207e2f23`. Grading requires it present (gold-span retrieval checks read source files).
- Toolchain: uv only (`uv add`, `uv run`, `uv run pytest`). Never pip, never bare `python`.
- All work on branch `eval/rerun-v0.2.0-campaign`.

**Warning for the implementing session:** the interactive hook `.claude/hooks/eval_subject_gate.py` denies Bash/Read/Grep/Glob calls whose text names the corpus or the answer-key files (`tasks.json`, `gold.sha256`, `gold.blake3`, `results*.json`, `protocol.json`, `schedule.json`, `attestations*.jsonl`, `METHODOLOGY.md`, `django-golden.md`, `RUBRIC.md`). Write/Edit are not hooked, and `uv run pytest <testfile>` command lines don't name those files — so create files with Write, run tests by test-file path, and never `cat`/Read the gold files. If a shell command must name one, prefix the whole command with `BENCH_GATE=off ` (must be the very first token).

**Package layout (all tasks):**

```
evals/agentlens_evals/
  pyproject.toml
  agentlens_evals/
    __init__.py
    paths.py        # locations + campaign shape (Task 1)
    matching.py     # verbatim copy of harness matcher v2 (Task 2)
    metering.py     # bench_tool.py port — the only door to the corpus (Task 3)
    subject.py      # binary install/verify with manifest (Task 4)
    dataset.py      # typed tasks + pydantic-evals Dataset (Task 5)
    grading.py      # grade.py per-run logic, ported (Task 6)
    evaluators.py   # pydantic-evals Evaluator wrappers (Task 6)
    worker.py       # one claude-agent-sdk benchmark session (Task 8)
    campaign.py     # schedule, resume, attestation, leak detection (Task 9)
    report.py       # aggregation, falsification, markdown (Task 10)
    cli.py          # install / run / status / grade / report (Task 10)
  tests/
    test_matching.py
    test_metering.py
    test_subject.py
    test_dataset.py
    test_grading.py
    test_parity.py      # verification gate 1 (needs runs_v2 + corpus)
    test_worker.py
    test_campaign.py
    test_report.py
```

---

### Task 1: Package scaffold and paths

**Files:**
- Create: `evals/agentlens_evals/pyproject.toml`
- Create: `evals/agentlens_evals/agentlens_evals/__init__.py`
- Create: `evals/agentlens_evals/agentlens_evals/paths.py`
- Create: `evals/agentlens_evals/tests/__init__.py` (empty)
- Modify: `.gitignore` (add `evals/bin/`)
- Test: `evals/agentlens_evals/tests/test_paths.py`

**Interfaces:**
- Produces: module constants every later task imports from `agentlens_evals.paths`: `REPO_ROOT: Path`, `PROJECT_ROOT: Path`, `HARNESS_ROOT: Path`, `BIN_ROOT: Path`, `DJANGO_ROOT: Path`, `AGENTLENS: Path | None`, `RUNS_ROOT: Path`, `ARMS: tuple[str, str, str]`, `TASK_COUNT = 18`, `REPETITIONS = 3`, `REPETITIONS_SCHEDULED = 5`, `EXPECTED_RUNS = 162`.

- [ ] **Step 1: Create the package**

Write `evals/agentlens_evals/pyproject.toml`:

```toml
[project]
name = "agentlens-evals"
version = "0.1.0"
description = "Formal, re-runnable benchmark campaigns for agentlens"
requires-python = ">=3.12"
dependencies = [
    "pydantic>=2",
    "pydantic-evals>=2.31,<3",
    "claude-agent-sdk",
    "tiktoken==0.8.0",
    "blake3",
]

[project.scripts]
agentlens-evals = "agentlens_evals.cli:main"

[dependency-groups]
dev = ["pytest>=8", "pytest-asyncio>=0.24"]

[tool.pytest.ini_options]
asyncio_mode = "auto"

[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"
```

Write `evals/agentlens_evals/agentlens_evals/__init__.py` with only a docstring:

```python
"""Formal, re-runnable benchmark campaigns for agentlens."""
```

Write empty `evals/agentlens_evals/tests/__init__.py`.

- [ ] **Step 2: Write paths.py**

```python
"""Where the module reads and writes, and the shape of a campaign.

Mirrors evals/harness/paths.py, which froze these decisions for the v1/v2
campaigns. Two deliberate differences: AGENTLENS has no default -- the binary
under test comes only from BENCH_AGENTLENS, which the campaign driver sets
from a hash-verified manifest, never from target/release -- and the frozen
harness directory is exposed read-only as HARNESS_ROOT because the campaign
inputs (tasks, gold hashes, protocol, schedule) stay there.
"""

from __future__ import annotations

import os
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent
PROJECT_ROOT = PACKAGE_ROOT.parent
REPO_ROOT = PROJECT_ROOT.parent.parent
HARNESS_ROOT = REPO_ROOT / "evals" / "harness"
BIN_ROOT = REPO_ROOT / "evals" / "bin"

# The subject corpus defaults *outside* the repository on purpose: a checkout
# of Django inside the tree would be indexed by `agentlens dead` and walked by
# `rg`, contaminating every arm.
DJANGO_ROOT = Path(
    os.environ.get("BENCH_DJANGO_ROOT", Path.home() / "dev" / "django-6.0.7")
).resolve()
_agentlens = os.environ.get("BENCH_AGENTLENS")
AGENTLENS = Path(_agentlens).resolve() if _agentlens else None
RUNS_ROOT = Path(
    os.environ.get("BENCH_RUNS_ROOT", REPO_ROOT / ".eval" / "runs_v2")
).resolve()

TASK_COUNT = 18
ARMS = ("agentlens", "baseline", "linerange")
# Pre-registered at 5; campaigns execute 3. The schedule file is never
# regenerated to match the execution -- that is the one edit a benchmark
# author must never make.
REPETITIONS_SCHEDULED = 5
REPETITIONS = 3
EXPECTED_RUNS = TASK_COUNT * REPETITIONS * len(ARMS)
```

- [ ] **Step 3: Write the failing test** — `evals/agentlens_evals/tests/test_paths.py`:

```python
from pathlib import Path

from agentlens_evals import paths


def test_repo_layout_resolves() -> None:
    assert paths.REPO_ROOT.name == "agentlens"
    assert paths.HARNESS_ROOT == paths.REPO_ROOT / "evals" / "harness"
    assert paths.HARNESS_ROOT.is_dir()
    assert paths.BIN_ROOT == paths.REPO_ROOT / "evals" / "bin"


def test_campaign_shape() -> None:
    assert paths.ARMS == ("agentlens", "baseline", "linerange")
    assert paths.EXPECTED_RUNS == 162
    assert paths.REPETITIONS_SCHEDULED == 5


def test_agentlens_binary_has_no_default() -> None:
    # target/release must never be consulted implicitly.
    assert paths.AGENTLENS is None or "target" not in paths.AGENTLENS.parts
```

- [ ] **Step 4: Install and run**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv sync && uv run pytest tests/test_paths.py -v
```

Expected: 3 passed. (If `uv sync` fails on `claude-agent-sdk` not existing under that name, check `uv pip index` alternatives — the package name is `claude-agent-sdk` per its docs; report rather than substituting.)

- [ ] **Step 5: Gitignore the binary dir and commit**

Append to `/home/rhawk/dev/agentlens/.gitignore` (after the `.eval/` block):

```
# Binary under test for eval campaigns: installed by `agentlens-evals install`,
# pinned by manifest.json (version + sha256). Never committed; never resolved
# from target/release at campaign time.
evals/bin/
```

```bash
git add .gitignore evals/agentlens_evals docs/superpowers/specs/2026-08-17-agentlens-evals-module-design.md
git commit -m "feat(evals): scaffold agentlens-evals package with pinned paths"
```

(The spec status-line edit is already in the working tree; this commit carries it.)

---

### Task 2: Matcher v2, verbatim

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/matching.py` (byte-identical copy of `evals/harness/matching.py`)
- Test: `evals/agentlens_evals/tests/test_matching.py`

**Interfaces:**
- Produces: `phrase_matches(answer: str, phrase: str) -> bool`, `fact_satisfied(answer: str, fact: dict) -> bool`, plus `stem`, `canonical`, `tokenize`, `window_size` — identical names and behavior to the harness module. Task 6's `grading.py` imports `fact_satisfied` from here.

- [ ] **Step 1: Copy the file unchanged**

```bash
cp /home/rhawk/dev/agentlens/evals/harness/matching.py /home/rhawk/dev/agentlens/evals/agentlens_evals/agentlens_evals/matching.py
```

No edits — the module has no imports beyond `re` and no path dependencies. Byte-identity is the point: `diff` must be empty.

```bash
diff /home/rhawk/dev/agentlens/evals/harness/matching.py /home/rhawk/dev/agentlens/evals/agentlens_evals/agentlens_evals/matching.py && echo IDENTICAL
```

Expected: `IDENTICAL`.

- [ ] **Step 2: Write behavior-pinning tests** — `evals/agentlens_evals/tests/test_matching.py`:

```python
from agentlens_evals.matching import fact_satisfied, phrase_matches


def test_equals_notation_matches_prose() -> None:
    assert phrase_matches(
        "the defaults are `sync_capable=True`", "sync_capable defaults to true"
    )


def test_synonym_class_collapses() -> None:
    assert phrase_matches("the output is not smaller", "not shorter")


def test_negation_parity_blocks_denial() -> None:
    assert not phrase_matches("does not skip compression", "skips compression")
    assert not phrase_matches("skips compression", "does not skip compression")


def test_fact_satisfied_any_of() -> None:
    fact = {"id": "f1", "any_of": ["runs in reverse order", "iterates backward"]}
    assert fact_satisfied("middleware iterates backward over the list", fact)
    assert not fact_satisfied("middleware runs forward", fact)
```

- [ ] **Step 3: Run**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_matching.py -v
```

Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/matching.py evals/agentlens_evals/tests/test_matching.py
git commit -m "feat(evals): port matcher v2 verbatim"
```

---

### Task 3: Metering gate port

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/metering.py`
- Test: `evals/agentlens_evals/tests/test_metering.py`

**Interfaces:**
- Produces: module runnable as `python -m agentlens_evals.metering RUN_ID TOOL ARGS...` with byte-identical behavior to `evals/harness/bench_tool.py`; importable names `CALL_CAP = 25`, `TOKEN_CAP = 60_000`, `ARM_TOOLS`, `validate_baseline`, `validate_linerange`, `validate_rg`, `cap_result`, `parse_run_id`. Task 8's worker prompt names this invocation; Task 9's campaign sets its env.

- [ ] **Step 1: Copy and adjust imports**

```bash
cp /home/rhawk/dev/agentlens/evals/harness/bench_tool.py /home/rhawk/dev/agentlens/evals/agentlens_evals/agentlens_evals/metering.py
```

Then make exactly two edits with the Edit tool:

Edit 1 — the import line. Old:

```python
from paths import AGENTLENS, ARMS, DJANGO_ROOT, REPETITIONS, RUNS_ROOT
```

New:

```python
from agentlens_evals.paths import AGENTLENS, ARMS, DJANGO_ROOT, REPETITIONS, RUNS_ROOT
```

Edit 2 — in `main()`, the agentlens-arm branch must refuse when no binary is pinned (harness had a `target/release` default; this module deliberately has none). Old:

```python
    else:
        if any(not safe_argument(arg) for arg in args):
            die("absolute and parent-traversal arguments are forbidden")
        command = [str(AGENTLENS), tool, *args]
```

New:

```python
    else:
        if AGENTLENS is None:
            die("BENCH_AGENTLENS is not set; refuse to guess the binary under test")
        if any(not safe_argument(arg) for arg in args):
            die("absolute and parent-traversal arguments are forbidden")
        command = [str(AGENTLENS), tool, *args]
```

Everything else — caps, allowlists, sed single-form regex, token capping, submit-once, transcript record shape — stays byte-identical.

- [ ] **Step 2: Write the gate tests** (verification gate 2) — `evals/agentlens_evals/tests/test_metering.py`. These run the real entrypoint as a subprocess with a temp corpus and temp runs root, exactly as a worker would:

```python
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
    ok = invoke(corpus, runs_root, "r1-linerange-M01", "sed", "-n", "1,5p", "pkg/mod.py")
    assert ok.returncode == 0 and "line 5" in ok.stdout
    for args in (
        ["-n", "1,5p;w /tmp/x", "pkg/mod.py"],   # command riding the range
        ["-n", "1,5p"],                            # missing file
        ["-i", "1,5p", "pkg/mod.py"],              # in-place edit
        ["-n", "e id", "pkg/mod.py"],              # shell escape
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
            stream.write(json.dumps({"call_index": index + 1, "result_tokens": 1}) + "\n")
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
```

- [ ] **Step 3: Run**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_metering.py -v
```

Expected: 8 passed. (`test_agentlens_arm_requires_pinned_binary` fails before Edit 2 is applied — apply edits first, then all pass; if you want the TDD red state, run the suite between Edit 1 and Edit 2.)

- [ ] **Step 4: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/metering.py evals/agentlens_evals/tests/test_metering.py
git commit -m "feat(evals): port the metering gate with a mandatory pinned binary"
```

---

### Task 4: Subject binary install and verification

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/subject.py`
- Test: `evals/agentlens_evals/tests/test_subject.py`

**Interfaces:**
- Produces: `SubjectError(RuntimeError)`; `binary_path(version: str) -> Path`; `manifest_path(version: str) -> Path`; `sha256_of(path: Path) -> str`; `write_manifest(version: str, binary: Path, source_commit: str, built_at: str) -> None`; `install(version: str) -> Path` (cargo build, copy, manifest); `verify(version: str) -> Path` (re-hash, refuse mismatch). Task 9's campaign calls `verify()` before dispatching anything.

- [ ] **Step 1: Write the failing tests** — `evals/agentlens_evals/tests/test_subject.py`:

```python
from pathlib import Path

import pytest

from agentlens_evals import paths, subject


@pytest.fixture()
def bin_root(tmp_path: Path, monkeypatch) -> Path:
    root = tmp_path / "bin"
    monkeypatch.setattr(subject, "BIN_ROOT", root)
    return root


def fake_install(bin_root: Path, version: str = "9.9.9") -> Path:
    binary = subject.binary_path(version)
    binary.parent.mkdir(parents=True)
    binary.write_bytes(b"#!/bin/sh\necho agentlens 9.9.9\n")
    subject.write_manifest(version, binary, "deadbeef", "2026-08-17T00:00:00+00:00")
    return binary


def test_verify_accepts_matching_hash(bin_root) -> None:
    binary = fake_install(bin_root)
    assert subject.verify("9.9.9") == binary


def test_verify_refuses_tampered_binary(bin_root) -> None:
    binary = fake_install(bin_root)
    binary.write_bytes(b"#!/bin/sh\necho tampered\n")
    with pytest.raises(subject.SubjectError, match="sha256 mismatch"):
        subject.verify("9.9.9")


def test_verify_refuses_missing_manifest(bin_root) -> None:
    binary = subject.binary_path("9.9.9")
    binary.parent.mkdir(parents=True)
    binary.write_bytes(b"x")
    with pytest.raises(subject.SubjectError, match="manifest"):
        subject.verify("9.9.9")


def test_verify_refuses_version_mismatch(bin_root) -> None:
    fake_install(bin_root, "9.9.9")
    with pytest.raises(subject.SubjectError):
        subject.verify("8.8.8")
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_subject.py -v
```

Expected: FAIL — `ModuleNotFoundError` / `AttributeError` on `agentlens_evals.subject`.

- [ ] **Step 3: Write subject.py**

```python
"""The binary under test, pinned by version and hash.

The v0.2.0 campaign was contaminated because the wrapper resolved
target/release/agentlens, a path any `cargo build` mutates. Here a campaign
binary lives in gitignored evals/bin/<version>/ beside a manifest recording
what it is, and every campaign command re-hashes it and refuses on mismatch.
"""

from __future__ import annotations

import hashlib
import json
import shutil
import subprocess
from datetime import datetime, timezone
from pathlib import Path

from agentlens_evals.paths import BIN_ROOT, REPO_ROOT


class SubjectError(RuntimeError):
    """The binary under test cannot be trusted; refuse to run."""


def binary_path(version: str) -> Path:
    return BIN_ROOT / version / "agentlens"


def manifest_path(version: str) -> Path:
    return BIN_ROOT / version / "manifest.json"


def sha256_of(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_manifest(version: str, binary: Path, source_commit: str, built_at: str) -> None:
    manifest_path(version).write_text(
        json.dumps(
            {
                "version": version,
                "sha256": sha256_of(binary),
                "source_commit": source_commit,
                "built_at": built_at,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )


def install(version: str) -> Path:
    subprocess.run(["cargo", "build", "--release"], cwd=REPO_ROOT, check=True)
    built = REPO_ROOT / "target" / "release" / "agentlens"
    reported = subprocess.run(
        [str(built), "--version"], capture_output=True, text=True, check=True
    ).stdout.strip()
    if version not in reported:
        raise SubjectError(f"built binary reports {reported!r}, expected version {version}")
    commit = subprocess.run(
        ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, capture_output=True, text=True, check=True
    ).stdout.strip()
    destination = binary_path(version)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(built, destination)
    write_manifest(version, destination, commit, datetime.now(timezone.utc).isoformat())
    return destination


def verify(version: str) -> Path:
    binary = binary_path(version)
    manifest_file = manifest_path(version)
    if not binary.is_file():
        raise SubjectError(f"no installed binary for {version}: run `agentlens-evals install`")
    if not manifest_file.is_file():
        raise SubjectError(f"missing manifest for {version}")
    manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
    if manifest.get("version") != version:
        raise SubjectError(
            f"manifest records version {manifest.get('version')!r}, expected {version!r}"
        )
    actual = sha256_of(binary)
    if actual != manifest.get("sha256"):
        raise SubjectError(
            f"sha256 mismatch for {binary}: manifest {manifest.get('sha256')}, actual {actual}"
        )
    return binary
```

Note `install`/`verify` reference module attribute `BIN_ROOT` via the from-import; tests monkeypatch `subject.BIN_ROOT`, so `binary_path`/`manifest_path` must read it as a module global — the from-import binds it as one, and monkeypatching `subject.BIN_ROOT` rebinds exactly that global. This works as written.

- [ ] **Step 4: Run to verify pass**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_subject.py -v
```

Expected: 4 passed. (This is verification gate 3.)

- [ ] **Step 5: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/subject.py evals/agentlens_evals/tests/test_subject.py
git commit -m "feat(evals): hash-pinned subject binary with refuse-on-mismatch"
```

---

### Task 5: Typed tasks and the pydantic-evals dataset

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/dataset.py`
- Test: `evals/agentlens_evals/tests/test_dataset.py`

**Interfaces:**
- Consumes: `paths.HARNESS_ROOT` (Task 1).
- Produces: pydantic models `GoldAddress(address, start_line, end_line)`, `Fact(id, any_of)`, `BenchTask(id, type, prompt, required_addresses, supporting_addresses, required_facts, forbidden_claims)`; `verify_gold() -> bytes` (raises `GoldError` on hash mismatch); `load_tasks() -> list[BenchTask]`; `tasks_by_id() -> dict[str, BenchTask]`; `build_dataset(tasks, arm: str, repetition: int, task_order: list[str] | None) -> Dataset` where each `Case` has `name=<task id>`, `inputs=<run_id str>`, `metadata=<task model_dump dict>`. Tasks 6/8/9/10 consume `BenchTask` dicts via `.model_dump()`.

- [ ] **Step 1: Write the failing tests** — `evals/agentlens_evals/tests/test_dataset.py`:

```python
import shutil

import pytest

from agentlens_evals import dataset, paths


def test_loads_the_frozen_task_set() -> None:
    tasks = dataset.load_tasks()
    assert len(tasks) == paths.TASK_COUNT
    identifiers = [task.id for task in tasks]
    assert identifiers == [f"M{n:02d}" for n in range(1, 13)] + [
        f"L{n:02d}" for n in range(1, 7)
    ]
    assert {task.type for task in tasks} == {"comprehension", "localization"}
    for task in tasks:
        assert task.required_addresses, task.id
        if task.type == "localization":
            assert not task.required_facts, task.id


def test_gold_hash_mismatch_refuses(tmp_path, monkeypatch) -> None:
    fake = tmp_path / "harness"
    fake.mkdir()
    for name in ("tasks.json", "gold.sha256", "gold.blake3"):
        shutil.copy2(paths.HARNESS_ROOT / name, fake / name)
    (fake / "tasks.json").write_text(
        (fake / "tasks.json").read_text(encoding="utf-8") + "\n", encoding="utf-8"
    )
    monkeypatch.setattr(dataset, "HARNESS_ROOT", fake)
    with pytest.raises(dataset.GoldError):
        dataset.verify_gold()


def test_dataset_one_case_per_task_in_schedule_order() -> None:
    tasks = dataset.load_tasks()
    order = [task.id for task in reversed(tasks)]
    built = dataset.build_dataset(tasks, arm="baseline", repetition=2, task_order=order)
    assert [case.name for case in built.cases] == order
    assert built.cases[0].inputs == f"r2-baseline-{order[0]}"
    assert built.cases[0].metadata["id"] == order[0]
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_dataset.py -v
```

Expected: FAIL with `ModuleNotFoundError: agentlens_evals.dataset`.

- [ ] **Step 3: Write dataset.py**

```python
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
```

Caution: `verify_gold` must reference `HARNESS_ROOT` — for the monkeypatch in the test to work, change the from-import to `from agentlens_evals import paths` style? No: the test monkeypatches `dataset.HARNESS_ROOT`, and the from-import creates exactly that module global. As written it works.

- [ ] **Step 4: Run to verify pass**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_dataset.py -v
```

Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/dataset.py evals/agentlens_evals/tests/test_dataset.py
git commit -m "feat(evals): typed task models over the hash-verified gold set"
```

---

### Task 6: Grading port and pydantic-evals evaluators

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/grading.py`
- Create: `evals/agentlens_evals/agentlens_evals/evaluators.py`
- Test: `evals/agentlens_evals/tests/test_grading.py`

**Interfaces:**
- Consumes: `matching.fact_satisfied` (Task 2), `paths.DJANGO_ROOT`.
- Produces (grading.py): constants `MATCHER_VERSION = 2`, `REFERENCE_ARM = "agentlens"`, `HEDGES`, `LINE_REFERENCE`; functions `normalize`, `is_negated`, `contains_asserted`, `selector_name`, `address_found(answer, gold_dict) -> bool`, `forbidden_penalty(answer, claims) -> tuple[float, list[str]]`, `source_lines(gold_dict) -> set[str]`, `call_retrieves(record, arm, golds) -> bool`, `quartiles(values) -> dict` — all ported verbatim from `evals/harness/grade.py` lines 25–147 and 237–247; dataclass `RunArtifacts(run_id, arm, answer, transcript: list[dict], capped: bool, run_dir: Path)`; `load_run(run_id: str, runs_root: Path) -> RunArtifacts`; `grade_artifacts(task: dict, artifacts: RunArtifacts) -> dict` producing exactly the dict `grade_run` produced; `grade_run(task: dict, arm: str, run_dir: Path) -> dict` kept as a thin wrapper for parity testing.
- Produces (evaluators.py): `BenchmarkScores(Evaluator)` whose `evaluate(ctx)` returns a mapping `{"score", "address_score", "fact_score", "penalty", "navigation", "tool_calls", "tool_result_tokens", "capped"}` computed via `grade_artifacts(ctx.metadata, ctx.output)`.

- [ ] **Step 1: Port the pure grading functions**

Create `grading.py`. The body of every ported function is copied character-for-character from `evals/harness/grade.py`; only the surrounding module changes. Full file:

```python
"""Per-run grading, ported verbatim from the frozen harness grade.py.

Matcher version 2. Any behavioral difference from evals/harness/grade.py on
the runs_v2 corpus is a port bug (verification gate 1) -- do not "improve"
anything here.
"""

from __future__ import annotations

import json
import re
import statistics
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from agentlens_evals.matching import fact_satisfied
from agentlens_evals.paths import DJANGO_ROOT

MATCHER_VERSION = 2
REFERENCE_ARM = "agentlens"
HEDGES = ("might", "possibly", "i'm not sure", "i am not sure", "appears to", "may")

LINE_REFERENCE = re.compile(
    r"\bL(?P<start_l>\d+)(?:\s*-\s*L?(?P<end_l>\d+))?\b"
    r"|:(?P<start_colon>\d+)(?:-(?P<end_colon>\d+))?"
    r"|\blines?\s+(?P<start_word>\d+)(?:\s*(?:-|–|—|to)\s*(?P<end_word>\d+))?\b",
    flags=re.IGNORECASE,
)
```

Then append, copied verbatim from `evals/harness/grade.py` (the implementer copies these bodies from that file, lines 45–180 and 237–247, without modification): `normalize`, `is_negated`, `contains_asserted`, `selector_name`, `address_found`, `forbidden_penalty`, `source_lines`, `call_retrieves`, `quartiles`. The only textual difference allowed is none — same signatures, same bodies. Verify with:

```bash
BENCH_GATE=off sh -c 'for fn in normalize is_negated contains_asserted selector_name address_found forbidden_penalty source_lines call_retrieves quartiles; do diff <(sed -n "/^def $fn(/,/^def \|^$/p" evals/harness/grade.py) <(sed -n "/^def $fn(/,/^def \|^$/p" evals/agentlens_evals/agentlens_evals/grading.py) >/dev/null || echo "DIFFERS: $fn"; done; echo checked'
```

Expected: `checked` with no `DIFFERS:` lines (blank-line trailing differences are acceptable; the parity test in Task 7 is the real gate).

Then append the run-loading layer and the graded-run function (this is `grade_run` from the harness, split so evaluators can score an already-loaded run; the arithmetic is identical):

```python
@dataclass
class RunArtifacts:
    run_id: str
    arm: str
    answer: str
    transcript: list[dict[str, Any]]
    capped: bool
    run_dir: Path


def load_run(run_id: str, runs_root: Path) -> RunArtifacts:
    repetition, arm, task_id = run_id.split("-", 2)
    run_dir = runs_root / f"repetition-{repetition[1:]}" / arm / task_id
    answer_path = run_dir / "answer.txt"
    if not answer_path.exists():
        raise FileNotFoundError(f"missing answer: {answer_path}")
    transcript_path = run_dir / "transcript.jsonl"
    transcript = [
        json.loads(line)
        for line in transcript_path.read_text(encoding="utf-8").splitlines()
        if line
    ]
    return RunArtifacts(
        run_id=run_id,
        arm=arm,
        answer=answer_path.read_text(encoding="utf-8"),
        transcript=transcript,
        capped=(run_dir / "capped").exists(),
        run_dir=run_dir,
    )


def grade_artifacts(task: dict[str, Any], artifacts: RunArtifacts) -> dict[str, Any]:
    answer = artifacts.answer
    required_found = [
        gold["address"] for gold in task["required_addresses"] if address_found(answer, gold)
    ]
    supporting_found = [
        gold["address"] for gold in task["supporting_addresses"] if address_found(answer, gold)
    ]
    address_score = min(
        1.0,
        (len(required_found) + 0.5 * len(supporting_found)) / len(task["required_addresses"]),
    )
    facts_found = [fact["id"] for fact in task["required_facts"] if fact_satisfied(answer, fact)]
    fact_score = len(facts_found) / len(task["required_facts"]) if task["required_facts"] else None
    penalty, forbidden_found = forbidden_penalty(answer, task["forbidden_claims"])
    raw = (
        address_score
        if task["type"] == "localization"
        else 0.5 * address_score + 0.5 * float(fact_score)
    )
    score = max(0.0, raw - penalty)
    navigation = None
    for record in artifacts.transcript:
        if call_retrieves(record, artifacts.arm, task["required_addresses"]):
            navigation = int(record["call_index"])
            break
    return {
        "task_id": task["id"],
        "arm": artifacts.arm,
        "answer": answer.rstrip(),
        "address_score": address_score,
        "fact_score": fact_score,
        "penalty": penalty,
        "score": score,
        "required_addresses_found": required_found,
        "supporting_addresses_found": supporting_found,
        "facts_found": facts_found,
        "forbidden_claims_found": forbidden_found,
        "tool_calls": len(artifacts.transcript),
        "tool_result_tokens": sum(int(r["result_tokens"]) for r in artifacts.transcript),
        "navigation": navigation,
        "capped": artifacts.capped,
    }


def grade_run(task: dict[str, Any], arm: str, run_dir: Path) -> dict[str, Any]:
    """Harness-shaped entry point: same signature and output as grade.py's."""
    answer_path = run_dir / "answer.txt"
    if not answer_path.exists():
        raise FileNotFoundError(f"missing answer: {answer_path}")
    transcript_path = run_dir / "transcript.jsonl"
    transcript = [
        json.loads(line)
        for line in transcript_path.read_text(encoding="utf-8").splitlines()
        if line
    ]
    artifacts = RunArtifacts(
        run_id=f"-{arm}-{task['id']}",
        arm=arm,
        answer=answer_path.read_text(encoding="utf-8"),
        transcript=transcript,
        capped=(run_dir / "capped").exists(),
        run_dir=run_dir,
    )
    return grade_artifacts(task, artifacts)
```

(`grade_run` must keep exactly this signature and returned dict — the parity gate in Task 7 compares it field-for-field against the harness original.)

- [ ] **Step 2: Write evaluators.py**

```python
"""Graders as pydantic-evals evaluators.

Thin wrappers: all scoring arithmetic lives in grading.py, which is the
verbatim grade.py port. An evaluator that recomputed anything would be a
second implementation to keep in sync -- exactly what this module exists to
avoid.
"""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any

from pydantic_evals.evaluators import Evaluator, EvaluatorContext

from agentlens_evals.grading import RunArtifacts, grade_artifacts

# Navigation is reported as call index 1..25; a run that never retrieved gold
# is charged one past the call cap, matching grade.py's aggregation.
NAVIGATION_CAP_SENTINEL = 26.0


@dataclass
class BenchmarkScores(Evaluator[str, RunArtifacts, dict[str, Any]]):
    def evaluate(self, ctx: EvaluatorContext[str, RunArtifacts, dict[str, Any]]) -> dict:
        graded = grade_artifacts(ctx.metadata, ctx.output)
        fact_score = graded["fact_score"]
        navigation = graded["navigation"]
        return {
            "score": graded["score"],
            "address_score": graded["address_score"],
            "fact_score": -1.0 if fact_score is None else float(fact_score),
            "penalty": graded["penalty"],
            "navigation": (
                NAVIGATION_CAP_SENTINEL if navigation is None else float(navigation)
            ),
            "tool_calls": float(graded["tool_calls"]),
            "tool_result_tokens": float(graded["tool_result_tokens"]),
            "capped": graded["capped"],
        }
```

- [ ] **Step 3: Write the failing tests** — `evals/agentlens_evals/tests/test_grading.py`:

```python
from pathlib import Path

from agentlens_evals.grading import (
    RunArtifacts,
    address_found,
    call_retrieves,
    forbidden_penalty,
    grade_artifacts,
)

GOLD = {"address": "django/core/handlers/base.py#BaseHandler.get_response",
        "start_line": 140, "end_line": 165}

TASK = {
    "id": "T01",
    "type": "comprehension",
    "prompt": "irrelevant",
    "required_addresses": [GOLD],
    "supporting_addresses": [],
    "required_facts": [{"id": "f1", "any_of": ["returns the response"]}],
    "forbidden_claims": ["caches the response"],
}


def artifacts(answer: str, transcript=None, arm: str = "baseline") -> RunArtifacts:
    return RunArtifacts(
        run_id=f"r1-{arm}-T01", arm=arm, answer=answer,
        transcript=transcript or [], capped=False, run_dir=Path("/nonexistent"),
    )


def test_address_exact_and_windowed_forms() -> None:
    assert address_found("see django/core/handlers/base.py#BaseHandler.get_response", GOLD)
    assert address_found("in django/core/handlers/base.py, get_response does it", GOLD)
    assert address_found("django/core/handlers/base.py lines 150-160", GOLD)
    assert not address_found("django/core/handlers/base.py lines 400-420", GOLD)
    assert not address_found("django/urls/resolvers.py get_response", GOLD)


def test_score_formula_and_floor() -> None:
    graded = grade_artifacts(TASK, artifacts(
        "django/core/handlers/base.py#BaseHandler.get_response returns the response"))
    assert graded["address_score"] == 1.0 and graded["fact_score"] == 1.0
    assert graded["score"] == 1.0
    localization = dict(TASK, type="localization", required_facts=[])
    graded = grade_artifacts(localization, artifacts("no idea, maybe somewhere"))
    assert graded["score"] == 0.0


def test_forbidden_hedged_vs_flat() -> None:
    flat, matched = forbidden_penalty("it caches the response always", ["caches the response"])
    assert flat == 0.25 and matched == ["caches the response"]
    hedged, _ = forbidden_penalty(
        "it possibly caches the response", ["caches the response"])
    assert hedged == 0.1
    negated, matched = forbidden_penalty(
        "it does not cache anything; never caches the response", ["caches the response"])
    assert negated == 0.0 and matched == []


def test_navigation_sed_span_overlap() -> None:
    record = {"exit_code": 0, "stdout": "some source text", "stderr": "",
              "tool": "sed", "args": ["-n", "150,160p", "django/core/handlers/base.py"],
              "call_index": 3, "result_tokens": 10}
    assert call_retrieves(record, "linerange", [GOLD])
    miss = dict(record, args=["-n", "400,420p", "django/core/handlers/base.py"])
    assert not call_retrieves(miss, "linerange", [GOLD])


def test_navigation_cat_named_file() -> None:
    record = {"exit_code": 0, "stdout": "content", "stderr": "", "tool": "cat",
              "args": ["django/core/handlers/base.py"], "call_index": 1, "result_tokens": 5}
    assert call_retrieves(record, "baseline", [GOLD])


def test_evaluator_wraps_grading() -> None:
    from agentlens_evals.evaluators import BenchmarkScores
    from pydantic_evals.evaluators import EvaluatorContext

    ctx = EvaluatorContext(
        name="T01", inputs="r1-baseline-T01", metadata=TASK, expected_output=None,
        output=artifacts(
            "django/core/handlers/base.py#BaseHandler.get_response returns the response"),
        duration=0.0, _span_tree=None, attributes={}, metrics={},
    )
    scores = BenchmarkScores().evaluate(ctx)
    assert scores["score"] == 1.0 and scores["navigation"] == 26.0
```

If `EvaluatorContext(...)` construction fails on `_span_tree=None` (it is a private dataclass field), adapt the test to construct via keyword `_span_tree` per the installed dataclass signature — introspect with `uv run python -c "import dataclasses; from pydantic_evals.evaluators import EvaluatorContext; print([f.name for f in dataclasses.fields(EvaluatorContext)])"` and pass every required field. Do not skip the test.

- [ ] **Step 4: Run**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_grading.py -v
```

Expected: 6 passed. (`test_navigation_*` and `source_lines` paths don't touch the real corpus because `call_retrieves` for cat/sed checks paths and spans before reading files; only the rg fallback branch reads `source_lines`, which these tests avoid.)

- [ ] **Step 5: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/grading.py evals/agentlens_evals/agentlens_evals/evaluators.py evals/agentlens_evals/tests/test_grading.py
git commit -m "feat(evals): port matcher-v2 grading and expose it as pydantic-evals evaluators"
```

---

### Task 7: Verification gate 1 — matcher-port parity over runs_v2

**Files:**
- Test: `evals/agentlens_evals/tests/test_parity.py`

**Interfaces:**
- Consumes: `grading.grade_run` (Task 6); the frozen harness modules imported by file path; the completed `.eval/runs_v2` corpus; the Django corpus.

- [ ] **Step 1: Write the parity test**

```python
"""Verification gate 1: the port must reproduce grade.py per-run scores exactly.

Loads the frozen harness grade.py by file path and compares its grade_run
output against the ported grading.grade_run for all 162 runs_v2 runs. Any
divergence is a port bug until proven otherwise.
"""

from __future__ import annotations

import importlib.util
import json
import sys

import pytest

from agentlens_evals import grading, paths

RUNS_V2 = paths.REPO_ROOT / ".eval" / "runs_v2"

pytestmark = pytest.mark.skipif(
    not RUNS_V2.is_dir() or not paths.DJANGO_ROOT.is_dir(),
    reason="requires the frozen runs_v2 corpus and the Django checkout",
)


def load_harness_module(name: str):
    if str(paths.HARNESS_ROOT) not in sys.path:
        sys.path.insert(0, str(paths.HARNESS_ROOT))
    spec = importlib.util.spec_from_file_location(name, paths.HARNESS_ROOT / f"{name}.py")
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
                assert actual == expected, f"divergence at r{repetition}-{arm}-{task_id}"
                compared += 1
    assert compared == paths.EXPECTED_RUNS
```

Notes for the implementer:
- The harness `grade.py` imports `matching` and `paths` from its own directory; the `sys.path.insert` handles that. It also imports `blake3` and `tiktoken` — both are dependencies of this package, so the harness modules resolve inside our venv.
- The harness `paths.RUNS_ROOT` default is already `.eval/runs_v2`; do not set `BENCH_RUNS_ROOT` when running this test.

- [ ] **Step 2: Run the gate**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_parity.py -v
```

Expected: 1 passed (162 runs compared). If it fails, the assertion message names the first divergent run: diff the two dicts for that run and fix the port — never adjust the harness.

- [ ] **Step 3: Commit**

```bash
git add evals/agentlens_evals/tests/test_parity.py
git commit -m "test(evals): matcher-port parity gate over the frozen runs_v2 corpus"
```

---

### Task 8: Worker — one benchmark session via claude-agent-sdk

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/worker.py`
- Test: `evals/agentlens_evals/tests/test_worker.py`

**Interfaces:**
- Consumes: `paths.PROJECT_ROOT`, `paths.REPO_ROOT`, `paths.DJANGO_ROOT`; `dataset.BenchTask` dicts.
- Produces: `WORKER_MODEL = "claude-haiku-4-5-20251001"`; `WorkerModelError(RuntimeError)`; `ARM_INTERFACE: dict[str, str]`; `wrapper_invocation(run_id: str) -> str`; `render_prompt(run_id: str, task_prompt: str) -> str`; `command_permitted(command: str, run_id: str) -> bool`; `permission_callback(run_id)` returning an async `can_use_tool` function; `worker_options(run_id, runs_root: Path, binary: Path) -> ClaudeAgentOptions`; `async dispatch(run_id: str, prompt: str, options) -> str | None` (raises `WorkerModelError` before any network call if `options.model != WORKER_MODEL`). Task 9 calls `render_prompt`, `worker_options`, `dispatch`.

- [ ] **Step 1: Write the failing tests** — `evals/agentlens_evals/tests/test_worker.py`:

```python
import pytest

from agentlens_evals import worker

RUN = "r1-linerange-M03"


def test_prompt_carries_isolation_interface_and_submit_contract() -> None:
    prompt = worker.render_prompt(RUN, "What does BaseHandler do?")
    assert "one independent benchmark worker" in prompt
    assert "sed line-range reads" in prompt          # linerange arm interface
    assert worker.wrapper_invocation(RUN) in prompt
    assert "submit exactly once" in prompt
    assert "Task M03: What does BaseHandler do?" in prompt


def test_each_arm_gets_its_own_interface() -> None:
    assert "slice, map, find" in worker.render_prompt("r1-agentlens-M03", "q")
    assert "cat whole-file reads" in worker.render_prompt("r1-baseline-M03", "q")


def test_command_permitted_only_for_the_wrapper() -> None:
    good = worker.wrapper_invocation(RUN) + " rg get_response django"
    assert worker.command_permitted(good, RUN)
    submit = worker.wrapper_invocation(RUN) + " submit 'found; it & works | fine'"
    assert worker.command_permitted(submit, RUN)     # metacharacters inside quotes
    assert not worker.command_permitted("cat /etc/passwd", RUN)
    assert not worker.command_permitted(good + " && cat /etc/passwd", RUN)
    assert not worker.command_permitted(good + " ; rm -rf /", RUN)
    assert not worker.command_permitted(good + " > /tmp/exfil", RUN)
    assert not worker.command_permitted(good + " $(cat gold)", RUN)
    other_run = worker.wrapper_invocation("r1-baseline-M01") + " cat x.py"
    assert not worker.command_permitted(other_run, RUN)


async def test_permission_callback_denies_non_bash_tools() -> None:
    can_use = worker.permission_callback(RUN)
    verdict = await can_use("Read", {"file_path": "/etc/passwd"}, None)
    assert verdict.behavior == "deny"
    verdict = await can_use("Bash", {"command": "cat /etc/passwd"}, None)
    assert verdict.behavior == "deny"
    verdict = await can_use(
        "Bash", {"command": worker.wrapper_invocation(RUN) + " map ."}, None
    )
    assert verdict.behavior == "allow"


async def test_dispatch_refuses_any_model_but_haiku(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    options.model = "claude-fable-5"
    with pytest.raises(worker.WorkerModelError):
        await worker.dispatch(RUN, "prompt", options)


def test_worker_options_are_hermetic(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    assert options.model == worker.WORKER_MODEL
    assert options.setting_sources == []
    assert options.allowed_tools == []
    assert options.env["BENCH_RUNS_ROOT"] == str(tmp_path)
    assert options.env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] == "1"
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_worker.py -v
```

Expected: FAIL with `ModuleNotFoundError: agentlens_evals.worker`.

- [ ] **Step 3: Write worker.py**

```python
"""One benchmark session: a Haiku 4.5 worker that may touch only the wrapper.

The prompt semantics are ported from attest.py:render_prompt; the isolation
that eval_subject_gate.py provided for interactive dispatch is provided here
by the SDK permission callback -- Bash is the only tool, and the only Bash
command permitted is this run's own metering-wrapper invocation.
"""

from __future__ import annotations

import re
from pathlib import Path

from claude_agent_sdk import (
    ClaudeAgentOptions,
    PermissionResultAllow,
    PermissionResultDeny,
    ResultMessage,
    query,
)

from agentlens_evals.paths import DJANGO_ROOT, PROJECT_ROOT, REPO_ROOT

# Pinned by decision, not configuration: cost (a prior campaign was cut from 5
# to 3 repetitions over expense) and comparability (v1/v2 attestations record
# this model). Dispatch refuses anything else.
WORKER_MODEL = "claude-haiku-4-5-20251001"


class WorkerModelError(RuntimeError):
    """Dispatch was configured with a model other than the pinned worker model."""


ARM_INTERFACE = {
    "agentlens": (
        "Available research commands: slice, map, find, literals, callers, packet, and dead.\n"
    ),
    "baseline": (
        "Available research commands: rg matches and cat whole-file reads. "
        "Line-range and context reads are unavailable.\n"
    ),
    "linerange": (
        "Available research commands: rg matches and sed line-range reads, "
        "written exactly as `sed -n 'START,ENDp' FILE`. Whole-file reads are "
        "unavailable: read only the line ranges you need.\n"
    ),
}

# Shell metacharacters outside single quotes turn one wrapper call into two
# commands. Quoted answers may contain anything.
UNQUOTED_METACHARACTERS = re.compile(r"[;&|<>`\n]|\$\(")


def wrapper_invocation(run_id: str) -> str:
    return f"uv run --project {PROJECT_ROOT} python -m agentlens_evals.metering {run_id}"


def render_prompt(run_id: str, task_prompt: str) -> str:
    _repetition, arm, task_id = run_id.split("-", 2)
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
    command = (
        f"\nInvoke research as:\n{wrapper_invocation(run_id)} COMMAND ARGS...\n\n"
        "After composing the answer, submit exactly once as:\n"
        f"{wrapper_invocation(run_id)} submit 'COMPLETE ANSWER'\n\n"
        "Then return only `submitted` as your final response.\n\n"
        f"Task {task_id}: {task_prompt}\n"
    )
    return common + ARM_INTERFACE[arm] + command


def command_permitted(command: str, run_id: str) -> bool:
    stripped = command.strip()
    prefix = wrapper_invocation(run_id)
    if not (stripped == prefix or stripped.startswith(prefix + " ")):
        return False
    unquoted = re.sub(r"'[^']*'", "", stripped)
    return not UNQUOTED_METACHARACTERS.search(unquoted)


def permission_callback(run_id: str):
    async def can_use_tool(tool_name: str, input_data: dict, context):
        if tool_name == "Bash" and command_permitted(str(input_data.get("command", "")), run_id):
            return PermissionResultAllow()
        return PermissionResultDeny(
            message="only this run's metering-wrapper invocation is permitted",
            interrupt=False,
        )

    return can_use_tool


def worker_options(run_id: str, runs_root: Path, binary: Path) -> ClaudeAgentOptions:
    return ClaudeAgentOptions(
        model=WORKER_MODEL,
        allowed_tools=[],  # nothing auto-approved: every call reaches the callback
        can_use_tool=permission_callback(run_id),
        permission_mode="default",
        cwd=str(REPO_ROOT),
        setting_sources=[],  # no CLAUDE.md, no hooks, no user settings
        max_turns=60,
        env={
            "BENCH_RUNS_ROOT": str(runs_root),
            "BENCH_AGENTLENS": str(binary),
            "BENCH_DJANGO_ROOT": str(DJANGO_ROOT),
            "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1",
        },
    )


async def dispatch(run_id: str, prompt: str, options: ClaudeAgentOptions) -> str | None:
    if options.model != WORKER_MODEL:
        raise WorkerModelError(
            f"worker model must be {WORKER_MODEL}, got {options.model!r}"
        )
    async for message in query(prompt=prompt, options=options):
        if isinstance(message, ResultMessage):
            return message.result if message.subtype == "success" else None
    return None
```

If any imported name differs in the installed `claude-agent-sdk` (e.g. `ResultMessage` lives in a submodule), fix the import by introspection (`uv run python -c "import claude_agent_sdk; print(sorted(dir(claude_agent_sdk)))"`) — do not change the module's behavior.

- [ ] **Step 4: Run to verify pass**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_worker.py -v
```

Expected: 6 passed. (This is verification gate 4 plus the SDK equivalent of the subject gate.)

- [ ] **Step 5: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/worker.py evals/agentlens_evals/tests/test_worker.py
git commit -m "feat(evals): sdk worker pinned to haiku with a wrapper-only permission gate"
```

---

### Task 9: Campaign orchestration — schedule, resume, attestation, leaks

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/campaign.py`
- Test: `evals/agentlens_evals/tests/test_campaign.py`

**Interfaces:**
- Consumes: `subject.verify` (Task 4), `dataset.tasks_by_id` (Task 5), `worker.render_prompt` / `worker_options` / `dispatch` (Task 8), `paths` constants.
- Produces: `scheduled_runs() -> list[str]` (schedule order, truncated to 3 executed repetitions); `run_directory(run_id, runs_root) -> Path`; `is_complete(run_id, runs_root) -> bool`; `campaign_status(runs_root) -> dict` with keys `complete`, `total`, `per_arm`, `stranded`, `pending`; attestation `read_events(runs_root)`, `record_start(runs_root, run_id, prompt)` (sequential-order enforced), `record_complete(runs_root, run_id)`; leak detection `unsupported_citations(run_id, runs_root) -> list[str] | None`, `classify(run_id, runs_root) -> str | None` (`"clean" | "memorisation" | "breach"`); `async run_campaign(version: str, runs_root: Path, concurrency: int = 4) -> dict`.

- [ ] **Step 1: Write the failing tests** — `evals/agentlens_evals/tests/test_campaign.py`:

```python
import json
from pathlib import Path

import pytest

from agentlens_evals import campaign, paths


def test_schedule_matches_the_frozen_harness_order() -> None:
    runs = campaign.scheduled_runs()
    assert len(runs) == paths.EXPECTED_RUNS
    assert len(set(runs)) == paths.EXPECTED_RUNS
    schedule = json.loads(
        (paths.HARNESS_ROOT / "schedule.json").read_text(encoding="utf-8")
    )["schedule"]
    first = schedule[0]
    assert runs[0] == f"r1-{first['arm_order'][0]}-{first['task_order'][0]}"
    assert all(run.startswith(("r1-", "r2-", "r3-")) for run in runs)


def seed_run(runs_root: Path, run_id: str, *, answer: str | None,
             transcript: list[dict] | None, capped: bool = False) -> Path:
    directory = campaign.run_directory(run_id, runs_root)
    directory.mkdir(parents=True, exist_ok=True)
    if transcript is not None:
        with (directory / "transcript.jsonl").open("w") as stream:
            for record in transcript:
                stream.write(json.dumps(record) + "\n")
    if answer is not None:
        (directory / "answer.txt").write_text(answer + "\n", encoding="utf-8")
    if capped:
        (directory / "capped").touch()
    return directory


def test_status_counts_complete_stranded_pending(tmp_path) -> None:
    runs = campaign.scheduled_runs()
    seed_run(tmp_path, runs[0], answer="done", transcript=[{"result_tokens": 1}])
    seed_run(tmp_path, runs[1], answer=None, transcript=[{"result_tokens": 1}])
    status = campaign.campaign_status(tmp_path)
    assert status["complete"] == 1
    assert status["stranded"] == [runs[1]]
    assert status["pending"] == paths.EXPECTED_RUNS - 1
    assert status["total"] == paths.EXPECTED_RUNS


def test_attestation_start_order_is_enforced(tmp_path) -> None:
    runs = campaign.scheduled_runs()
    campaign.record_start(tmp_path, runs[0], "prompt zero")
    with pytest.raises(SystemExit, match="run order mismatch"):
        campaign.record_start(tmp_path, runs[5], "out of order")
    campaign.record_start(tmp_path, runs[1], "prompt one")
    events = campaign.read_events(tmp_path)
    assert [event["run_id"] for event in events] == runs[:2]
    assert all(event["model"] == "claude-haiku-4-5-20251001" for event in events)


def test_leak_classification(tmp_path) -> None:
    runs = campaign.scheduled_runs()
    transcript = [{"stdout": "django/core/handlers/base.py:1:x", "stderr": "", "args": []}]
    seed_run(tmp_path, runs[0],
             answer="see django/core/handlers/base.py", transcript=transcript)
    assert campaign.classify(runs[0], tmp_path) == "clean"
    seed_run(tmp_path, runs[1],
             answer="see django/urls/resolvers.py", transcript=transcript)
    assert campaign.classify(runs[1], tmp_path) == "breach"
    seed_run(tmp_path, runs[2],
             answer="see django/urls/resolvers.py", transcript=transcript, capped=True)
    assert campaign.classify(runs[2], tmp_path) == "memorisation"
    seed_run(tmp_path, runs[3], answer=None, transcript=transcript)
    assert campaign.classify(runs[3], tmp_path) is None


async def test_run_campaign_dispatches_only_outstanding(tmp_path, monkeypatch) -> None:
    runs = campaign.scheduled_runs()
    for run_id in runs[2:]:
        seed_run(tmp_path, run_id, answer="already done",
                 transcript=[{"stdout": "", "stderr": "", "args": []}])
    monkeypatch.setattr(campaign.subject, "verify", lambda version: Path("/fake/agentlens"))

    dispatched: list[str] = []

    async def fake_dispatch(run_id, prompt, options):
        dispatched.append(run_id)
        seed_run(tmp_path, run_id, answer="worker answer",
                 transcript=[{"stdout": "", "stderr": "", "args": []}])
        return "submitted"

    monkeypatch.setattr(campaign.worker, "dispatch", fake_dispatch)
    summary = await campaign.run_campaign("9.9.9", tmp_path, concurrency=2)
    assert sorted(dispatched) == sorted(runs[:2])
    assert summary["dispatched"] == 2
    assert summary["breaches"] == []
```

- [ ] **Step 2: Run to verify failure**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_campaign.py -v
```

Expected: FAIL with `ModuleNotFoundError: agentlens_evals.campaign`.

- [ ] **Step 3: Write campaign.py**

```python
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
from agentlens_evals.paths import ARMS, EXPECTED_RUNS, HARNESS_ROOT, REPETITIONS

CITED_PATH = re.compile(r"\b((?:[\w.-]+/)+[\w.-]+\.py)\b")


def scheduled_runs() -> list[str]:
    schedule = json.loads(
        (HARNESS_ROOT / "schedule.json").read_text(encoding="utf-8")
    )["schedule"]
    runs: list[str] = []
    for repetition in schedule[:REPETITIONS]:
        for task_id in repetition["task_order"]:
            for arm in repetition["arm_order"]:
                runs.append(f"r{repetition['repetition']}-{arm}-{task_id}")
    return runs


def run_directory(run_id: str, runs_root: Path) -> Path:
    repetition, arm, task_id = run_id.split("-", 2)
    return runs_root / f"repetition-{repetition[1:]}" / arm / task_id


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
        arm: sum(1 for run in complete if run.split("-", 2)[1] == arm) for arm in ARMS
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
    expected = [run for run in scheduled_runs() if run not in
                {event["run_id"] for event in started}]
    if not expected or run_id != expected[0]:
        wanted = expected[0] if expected else "<none>"
        raise SystemExit(f"run order mismatch: expected {wanted}, got {run_id}")
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
        raise SystemExit(f"incomplete artifacts for {run_id}")
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
        raise SystemExit(f"schedule yields {len(runs)} runs, expected {EXPECTED_RUNS}")
    tasks = tasks_by_id()
    outstanding = [run for run in runs if not is_complete(run, runs_root)]

    started = {event["run_id"] for event in read_events(runs_root)
               if event["event"] == "start"}
    semaphore = asyncio.Semaphore(concurrency)

    async def execute(run_id: str) -> None:
        _repetition, _arm, task_id = run_id.split("-", 2)
        prompt = worker.render_prompt(run_id, tasks[task_id].prompt)
        options = worker.worker_options(run_id, runs_root, binary)
        async with semaphore:
            await worker.dispatch(run_id, prompt, options)

    # Start events append sequentially in schedule order before any dispatch
    # completes out of order; a resumed run keeps its original start event.
    pending: list[asyncio.Task] = []
    for run_id in outstanding:
        if run_id not in started:
            _repetition, _arm, task_id = run_id.split("-", 2)
            record_start(runs_root, run_id,
                         worker.render_prompt(run_id, tasks[task_id].prompt))
        pending.append(asyncio.ensure_future(execute(run_id)))
    if pending:
        await asyncio.gather(*pending)

    completed_events = {event["run_id"] for event in read_events(runs_root)
                        if event["event"] == "complete"}
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
    }
```

Design note on `record_start`: the order check must tolerate resume — on a resumed campaign the already-started runs are not re-started, so the "next expected" run is the first scheduled run without a start event. The implementation computes exactly that (`expected` = scheduled runs minus started). This intentionally relaxes attest.py's strict prefix check just enough to allow resumption while still refusing out-of-order starts; `test_attestation_start_order_is_enforced` pins the behavior.

- [ ] **Step 4: Run to verify pass**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest tests/test_campaign.py -v
```

Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/campaign.py evals/agentlens_evals/tests/test_campaign.py
git commit -m "feat(evals): schedule-ordered campaign driver with resume, attestation, leak checks"
```

---

### Task 10: Report, CLI, and end-to-end regrade

**Files:**
- Create: `evals/agentlens_evals/agentlens_evals/report.py`
- Create: `evals/agentlens_evals/agentlens_evals/cli.py`
- Test: `evals/agentlens_evals/tests/test_report.py`

**Interfaces:**
- Consumes: `grading.grade_run` / `quartiles` / `REFERENCE_ARM` / `MATCHER_VERSION` (Task 6), `dataset.verify_gold` / `load_tasks` (Task 5), `evaluators.BenchmarkScores` + `dataset.build_dataset` + `grading.load_run` (derived pydantic-evals report), `campaign` (Task 9), `subject` (Task 4).
- Produces: `grade_campaign(runs_root: Path, repetitions: int, arms: list[str]) -> dict` — the exact aggregation `grade.py:main` produced (`arms`, `comparisons`, `per_task`, `runs`, `benchmark_failed`, hashes, matcher version); `write_results(results: dict, runs_root: Path) -> Path` (writes `<runs_root>/results.json`); `render_markdown(results: dict) -> str`; `evaluation_reports(runs_root, repetitions, arms)` — per arm×repetition `Dataset.evaluate_sync` passes whose task callable loads runs from disk (derived artifact); `cli.main()` with subcommands `install`, `run`, `status`, `grade`, `report`.

- [ ] **Step 1: Write report.py**

The aggregation is `grade.py:main` (lines 271–422 of the harness file) ported into a pure function. Copy the aggregation logic verbatim with these mechanical substitutions and no others:
- `TASKS`, gold-hash check → `dataset.verify_gold()` + `dataset.load_tasks()` dumps (`[task.model_dump() for task in load_tasks()]`).
- `RUNS_ROOT / ...` → the `runs_root` parameter.
- `ROOT / "protocol.json"` → `HARNESS_ROOT / "protocol.json"` (read-only).
- Output writing moves to `write_results`; the function returns the `result` dict instead of writing.

Full file:

```python
"""Aggregation, falsification, and reports.

grade_campaign is the harness grade.py aggregation, ported: per-arm and
per-repetition quartiles, per-task head-to-head, and cost/accuracy
comparisons of agentlens against each control separately -- controls are
never pooled -- judged against the pre-registered thresholds in
protocol.json. The pydantic-evals passes in evaluation_reports are derived
artifacts on top of the same on-disk runs.
"""

from __future__ import annotations

import hashlib
import json
import statistics
from collections import defaultdict
from pathlib import Path
from typing import Any

from blake3 import blake3

from agentlens_evals import dataset as dataset_module
from agentlens_evals.dataset import build_dataset, load_tasks
from agentlens_evals.evaluators import BenchmarkScores
from agentlens_evals.grading import (
    MATCHER_VERSION,
    REFERENCE_ARM,
    grade_run,
    load_run,
    quartiles,
)
from agentlens_evals.paths import HARNESS_ROOT


def grade_campaign(runs_root: Path, repetitions: int, arms: list[str]) -> dict[str, Any]:
    if REFERENCE_ARM not in arms:
        raise SystemExit(f"{REFERENCE_ARM} must be among the graded arms")
    task_bytes = dataset_module.verify_gold()
    tasks = [task.model_dump() for task in load_tasks()]
    repetition_range = range(1, repetitions + 1)

    runs: list[dict[str, Any]] = []
    for repetition in repetition_range:
        for arm in arms:
            for task in tasks:
                run_dir = runs_root / f"repetition-{repetition}" / arm / task["id"]
                graded = grade_run(task, arm, run_dir)
                graded["repetition"] = repetition
                runs.append(graded)

    grouped: dict[tuple[str, str], list[dict[str, Any]]] = defaultdict(list)
    for run in runs:
        grouped[(run["arm"], run["task_id"])].append(run)
    per_task: list[dict[str, Any]] = []
    for task in tasks:
        row: dict[str, Any] = {"task_id": task["id"]}
        for arm in arms:
            arm_runs = grouped[(arm, task["id"])]
            row[arm] = {
                "score": quartiles([r["score"] for r in arm_runs]),
                "tool_result_tokens": quartiles(
                    [float(r["tool_result_tokens"]) for r in arm_runs]
                ),
                "navigation": quartiles(
                    [
                        float(r["navigation"] if r["navigation"] is not None else 26)
                        for r in arm_runs
                    ]
                ),
                "capped_runs": sum(bool(r["capped"]) for r in arm_runs),
            }
        per_task.append(row)

    repetition_metrics: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for arm in arms:
        for repetition in repetition_range:
            arm_runs = [
                run for run in runs if run["arm"] == arm and run["repetition"] == repetition
            ]
            total_points = sum(run["score"] for run in arm_runs)
            total_tokens = sum(run["tool_result_tokens"] for run in arm_runs)
            repetition_metrics[arm].append(
                {
                    "repetition": repetition,
                    "accuracy": statistics.mean(run["score"] for run in arm_runs),
                    "cost_tokens_per_point": (
                        total_tokens / total_points if total_points else None
                    ),
                    "navigation_median": statistics.median(
                        run["navigation"] if run["navigation"] is not None else 26
                        for run in arm_runs
                    ),
                    "total_points": total_points,
                    "total_tool_result_tokens": total_tokens,
                    "capped_runs": sum(bool(run["capped"]) for run in arm_runs),
                }
            )
    arm_summary: dict[str, Any] = {}
    for arm in arms:
        metrics = repetition_metrics[arm]
        arm_summary[arm] = {
            "accuracy": quartiles([metric["accuracy"] for metric in metrics]),
            "cost_tokens_per_point": quartiles(
                [metric["cost_tokens_per_point"] for metric in metrics]
            ),
            "navigation": quartiles([metric["navigation_median"] for metric in metrics]),
            "repetitions": metrics,
            "capped_runs": sum(metric["capped_runs"] for metric in metrics),
        }

    protocol = json.loads((HARNESS_ROOT / "protocol.json").read_text())
    thresholds = protocol["falsification"]
    comparisons: dict[str, Any] = {}
    for control in [arm for arm in arms if arm != REFERENCE_ARM]:
        head_to_head = {"agentlens_wins": 0, "control_wins": 0, "ties": 0}
        for row in per_task:
            left = row[REFERENCE_ARM]["score"]["median"]
            right = row[control]["score"]["median"]
            if left > right:
                head_to_head["agentlens_wins"] += 1
            elif left < right:
                head_to_head["control_wins"] += 1
            else:
                head_to_head["ties"] += 1
        indices = range(len(repetition_range))
        cost_ratio = quartiles(
            [
                repetition_metrics[REFERENCE_ARM][index]["cost_tokens_per_point"]
                / repetition_metrics[control][index]["cost_tokens_per_point"]
                for index in indices
            ]
        )
        accuracy_delta = quartiles(
            [
                repetition_metrics[REFERENCE_ARM][index]["accuracy"]
                - repetition_metrics[control][index]["accuracy"]
                for index in indices
            ]
        )
        limits = thresholds.get(f"vs_{control}", {})
        failures = {
            "cost_ratio_too_high": cost_ratio["median"] >= limits["maximum_cost_ratio"],
            "accuracy_below_control": (
                accuracy_delta["median"] < -limits["maximum_accuracy_delta_below_arm"]
            ),
            "too_many_tasks_lost": (
                head_to_head["control_wins"] > limits["maximum_tasks_lost"]
            ),
        }
        comparisons[control] = {
            "cost_ratio": cost_ratio,
            "accuracy_delta": accuracy_delta,
            "head_to_head": head_to_head,
            "thresholds": limits,
            "falsification": failures,
            "failed": any(failures.values()),
        }

    return {
        "subject": protocol["subject"],
        "matcher_version": MATCHER_VERSION,
        "repetitions_graded": list(repetition_range),
        "arms_graded": list(arms),
        "gold_blake3": blake3(task_bytes).hexdigest(),
        "gold_sha256": hashlib.sha256(task_bytes).hexdigest(),
        "arms": arm_summary,
        "comparisons": comparisons,
        "benchmark_failed": any(entry["failed"] for entry in comparisons.values()),
        "per_task": per_task,
        "runs": runs,
    }


def write_results(results: dict[str, Any], runs_root: Path) -> Path:
    path = runs_root / "results.json"
    path.write_text(
        json.dumps(results, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    return path


def render_markdown(results: dict[str, Any]) -> str:
    lines = [
        f"# Benchmark results — {results['subject']}",
        "",
        f"Matcher v{results['matcher_version']}; "
        f"repetitions {results['repetitions_graded']}; "
        f"benchmark_failed: **{results['benchmark_failed']}**",
        "",
        "| arm | accuracy (median) | tokens/point (median) | navigation (median) | capped |",
        "|---|---|---|---|---|",
    ]
    for arm, summary in results["arms"].items():
        lines.append(
            f"| {arm} | {summary['accuracy']['median']:.3f} "
            f"| {summary['cost_tokens_per_point']['median']:.0f} "
            f"| {summary['navigation']['median']:.1f} "
            f"| {summary['capped_runs']} |"
        )
    lines.append("")
    for control, entry in results["comparisons"].items():
        h2h = entry["head_to_head"]
        lines += [
            f"## agentlens vs {control}",
            "",
            f"- cost ratio median: {entry['cost_ratio']['median']:.3f}",
            f"- accuracy delta median: {entry['accuracy_delta']['median']:+.3f}",
            f"- head-to-head: {h2h['agentlens_wins']}W / {h2h['control_wins']}L / {h2h['ties']}T",
            f"- falsified: {entry['failed']}",
            "",
        ]
    return "\n".join(lines)


def evaluation_reports(runs_root: Path, repetitions: int, arms: list[str]):
    """Derived pydantic-evals reports, one evaluate pass per arm x repetition."""
    tasks = load_tasks()
    reports = []
    for repetition in range(1, repetitions + 1):
        for arm in arms:
            built = build_dataset(tasks, arm=arm, repetition=repetition)
            built = built.model_copy(
                update={"evaluators": [BenchmarkScores()]}
            )
            report = built.evaluate_sync(
                lambda run_id: load_run(run_id, runs_root),
                name=f"r{repetition}-{arm}",
                progress=False,
            )
            reports.append(report)
    return reports
```

If `Dataset.model_copy(update={"evaluators": ...})` does not attach evaluators on the installed pydantic-evals, use `built.add_evaluator(BenchmarkScores())` instead (both exist on 2.31.0; prefer `add_evaluator`).

- [ ] **Step 2: Write cli.py**

```python
"""Console entry point: install / run / status / grade / report."""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path

from agentlens_evals import campaign, report, subject
from agentlens_evals.paths import ARMS, REPETITIONS, RUNS_ROOT


def main() -> None:
    parser = argparse.ArgumentParser(prog="agentlens-evals", description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    install = commands.add_parser("install", help="build and pin the binary under test")
    install.add_argument("--version", required=True)

    run = commands.add_parser("run", help="execute a campaign")
    run.add_argument("--tool-version", required=True)
    run.add_argument("--runs-root", type=Path, default=RUNS_ROOT)
    run.add_argument("--concurrency", type=int, default=4)

    status = commands.add_parser("status", help="complete / stranded / pending")
    status.add_argument("--runs-root", type=Path, default=RUNS_ROOT)

    grade = commands.add_parser("grade", help="grade a campaign into results.json")
    grade.add_argument("--runs-root", type=Path, default=RUNS_ROOT)
    grade.add_argument("--repetitions", type=int, default=REPETITIONS)
    grade.add_argument("--arms", nargs="+", default=list(ARMS))

    rep = commands.add_parser("report", help="markdown + derived pydantic-evals reports")
    rep.add_argument("--runs-root", type=Path, default=RUNS_ROOT)
    rep.add_argument("--repetitions", type=int, default=REPETITIONS)
    rep.add_argument("--arms", nargs="+", default=list(ARMS))

    options = parser.parse_args()

    if options.command == "install":
        binary = subject.install(options.version)
        print(f"installed {binary}")
    elif options.command == "run":
        summary = asyncio.run(
            campaign.run_campaign(
                options.tool_version, options.runs_root, options.concurrency
            )
        )
        print(summary)
        if summary["breaches"]:
            sys.exit(1)
    elif options.command == "status":
        state = campaign.campaign_status(options.runs_root)
        print(f"complete   {state['complete']}/{state['total']}")
        for arm, count in state["per_arm"].items():
            print(f"  {arm:10s} {count}")
        if state["stranded"]:
            print(f"stranded (transcript, no answer): {len(state['stranded'])}")
            for run_id in state["stranded"]:
                print(f"  {run_id}")
    elif options.command == "grade":
        results = report.grade_campaign(
            options.runs_root, options.repetitions, options.arms
        )
        path = report.write_results(results, options.runs_root)
        print(f"wrote {path}; benchmark_failed={results['benchmark_failed']}")
    elif options.command == "report":
        results = report.grade_campaign(
            options.runs_root, options.repetitions, options.arms
        )
        print(report.render_markdown(results))
        for evaluation in report.evaluation_reports(
            options.runs_root, options.repetitions, options.arms
        ):
            evaluation.print()
```

- [ ] **Step 3: Write the tests** — `evals/agentlens_evals/tests/test_report.py`:

```python
import pytest

from agentlens_evals import paths, report

RUNS_V2 = paths.REPO_ROOT / ".eval" / "runs_v2"

needs_corpus = pytest.mark.skipif(
    not RUNS_V2.is_dir() or not paths.DJANGO_ROOT.is_dir(),
    reason="requires the frozen runs_v2 corpus and the Django checkout",
)


@needs_corpus
def test_regrade_of_runs_v2_passes_the_preregistered_thresholds() -> None:
    results = report.grade_campaign(RUNS_V2, paths.REPETITIONS, list(paths.ARMS))
    assert results["matcher_version"] == 2
    assert len(results["runs"]) == paths.EXPECTED_RUNS
    # The published v0.2.0 outcome: every falsification threshold passes.
    assert results["benchmark_failed"] is False
    for control in ("baseline", "linerange"):
        assert control in results["comparisons"]


@needs_corpus
def test_markdown_renders_all_arms_and_comparisons() -> None:
    results = report.grade_campaign(RUNS_V2, paths.REPETITIONS, list(paths.ARMS))
    markdown = report.render_markdown(results)
    for arm in paths.ARMS:
        assert arm in markdown
    assert "agentlens vs baseline" in markdown
    assert "agentlens vs linerange" in markdown


@needs_corpus
def test_derived_pydantic_evals_reports_cover_every_pass() -> None:
    reports = report.evaluation_reports(RUNS_V2, paths.REPETITIONS, list(paths.ARMS))
    assert len(reports) == paths.REPETITIONS * len(paths.ARMS)


def test_grade_requires_the_reference_arm(tmp_path) -> None:
    with pytest.raises(SystemExit, match="agentlens must be among"):
        report.grade_campaign(tmp_path, 1, ["baseline"])
```

- [ ] **Step 4: Run the whole suite and the CLI smoke**

```bash
cd /home/rhawk/dev/agentlens/evals/agentlens_evals && uv run pytest -v
uv run agentlens-evals status
```

Expected: all tests pass; `status` prints `complete   162/162` for the default runs_v2 root with per-arm counts of 54.

- [ ] **Step 5: Commit**

```bash
git add evals/agentlens_evals/agentlens_evals/report.py evals/agentlens_evals/agentlens_evals/cli.py evals/agentlens_evals/tests/test_report.py
git commit -m "feat(evals): campaign grading, falsification report, and cli"
```

---

## Facts, constraints, and decisions the implementer needs (no other context exists)

- **Decisions settled with the user (do not reopen):** port not wrap/rewrite; workers via Python claude-agent-sdk; worker model pinned `claude-haiku-4-5-20251001`; binary dir `evals/bin/<version>/` with manifest.
- **Sequential start-order kept** (spec default). Consequence: execution/dispatch is driven by the module in schedule order; pydantic-evals drives the *evaluation* passes over completed runs. This is the recorded resolution of the spec's "let it drive execution" phrasing — pydantic-evals cannot both own execution order per arm×repetition and honor the interleaved pre-registered schedule.
- **Resumability:** answer.txt present = complete, never re-dispatch (`submit` refuses to overwrite anyway); transcript without answer = stranded, re-dispatch resumes against the remaining call/token budget because the metering gate recounts prior transcript records.
- **pydantic-evals 2.31.0, verified by introspection:** `Case(name, inputs, metadata, expected_output, evaluators)`; `Dataset(name, cases, evaluators)` with `add_evaluator`, `evaluate_sync(task, name=, max_concurrency=, progress=, repeat=)` → `EvaluationReport` (`averages`, `print`, `render`); `Evaluator` abstract over `evaluate(ctx: EvaluatorContext) -> EvaluatorOutput` where output may be a scalar or a mapping; `EvaluatorContext` fields `name, inputs, metadata, expected_output, output, duration, _span_tree, attributes, metrics`.
- **claude-agent-sdk, verified via docs agent:** `query(prompt=..., options=ClaudeAgentOptions(...)) -> AsyncIterator[Message]`; `ClaudeAgentOptions(model, allowed_tools, can_use_tool, permission_mode, cwd, setting_sources, env, max_turns, system_prompt, max_budget_usd)`; tools listed in `allowed_tools` **skip** the `can_use_tool` callback (hence `allowed_tools=[]` here); `PermissionResultAllow()` / `PermissionResultDeny(message=, interrupt=)`; `ResultMessage(subtype, result, total_cost_usd, usage, num_turns)`; auth via `ANTHROPIC_API_KEY`; the SDK bundles its own Claude Code binary; `setting_sources=[]` disables CLAUDE.md/hooks/settings, `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1` disables auto-memory.
- **Ruled out:** regenerating `schedule.json` or `protocol.json` (pre-registration discipline); pooling controls in comparisons; resolving the binary from `target/release` (the v0.2.0 contamination); changing tokenizer or matcher behavior; new tasks/arms/rubric; CI integration (campaigns spend real API money — operator-initiated only).
- **Not investigated:** whether `claude-agent-sdk`'s `ClaudeAgentOptions` field set matches the docs agent's report exactly at the installed version — Task 8 tells the implementer to introspect and fix imports if names moved, without changing behavior. Also not investigated: `agentlens --version` output format (Task 4 checks `version in reported`, a substring, deliberately).
- **Known-good outcome for end-to-end checks:** the re-run v0.2.0 campaign (runs_v2, 162 runs) passes all pre-registered falsification thresholds; binary sha256 `77f8981529daafd1bd2263cc0adc1dd36f91d055cd819bbe6c9a2faaecdff5b4`.
- **Verification commands (confirmed shapes for this repo):** `uv sync`, `uv run pytest -v`, `uv run pytest tests/<file> -v`, all from `evals/agentlens_evals/`. The parity and report tests self-skip when `.eval/runs_v2` or `~/dev/django-6.0.7` are absent — on this machine both exist, so they must run and pass.
