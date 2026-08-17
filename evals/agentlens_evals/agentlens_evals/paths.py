"""Where the module reads and writes, and the shape of a campaign.

Mirrors evals/harness/paths.py, which froze these decisions for the v1/v2
campaigns. Two deliberate differences: AGENTLENS has no default -- the binary
under test comes only from BENCH_AGENTLENS, which the campaign driver sets
from a hash-verified manifest, never from target/release -- and the frozen
harness directory is exposed read-only as HARNESS_ROOT because the campaign
inputs (tasks, gold hashes, protocol, schedule) stay there.

DJANGO_ROOT defaults *outside* the repository on purpose: a checkout of
Django inside the tree would be indexed by `agentlens dead` and walked by
`rg`, contaminating every arm.

REPETITIONS_SCHEDULED is pre-registered at 5; campaigns execute REPETITIONS
(3). The schedule file is never regenerated to match the execution -- that
is the one edit a benchmark author must never make.
"""

from __future__ import annotations

import os
from pathlib import Path

PACKAGE_ROOT = Path(__file__).resolve().parent
PROJECT_ROOT = PACKAGE_ROOT.parent
REPO_ROOT = PROJECT_ROOT.parent.parent
HARNESS_ROOT = REPO_ROOT / "evals" / "harness"
BIN_ROOT = REPO_ROOT / "evals" / "bin"

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
REPETITIONS_SCHEDULED = 5
REPETITIONS = 3
EXPECTED_RUNS = TASK_COUNT * REPETITIONS * len(ARMS)
