"""Where the harness reads and writes, and the shape of a campaign.

Every script in this directory resolves the same three locations. They live
here so a machine with a different layout is one set of environment variables
away from running the benchmark, rather than a patch to each script -- the
v0.1.0 harness hardcoded them separately in three files, and all three went
stale together the moment the harness moved.
"""

from __future__ import annotations

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parent
REPO_ROOT = ROOT.parent.parent

# The subject corpus defaults *outside* the repository on purpose. A checkout of
# Django inside the tree would be indexed by `agentlens dead`, walked by `rg`,
# and would contaminate every arm.
DJANGO_ROOT = Path(
    os.environ.get("BENCH_DJANGO_ROOT", Path.home() / "dev" / "django-6.0.7")
).resolve()
AGENTLENS = Path(
    os.environ.get("BENCH_AGENTLENS", REPO_ROOT / "target" / "release" / "agentlens")
).resolve()
# The run corpus stays in the gitignored .eval/runs; only the harness is tracked.
RUNS_ROOT = Path(os.environ.get("BENCH_RUNS_ROOT", REPO_ROOT / ".eval" / "runs")).resolve()

TASK_COUNT = 18
REPETITIONS = 5
ARMS = ("agentlens", "baseline", "linerange")
EXPECTED_RUNS = TASK_COUNT * REPETITIONS * len(ARMS)
