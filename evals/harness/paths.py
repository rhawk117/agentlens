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
# The run corpus stays in the gitignored .eval/; only the harness is tracked.
#
# Campaigns get separate directories, and this is not cosmetic. Both campaigns
# use the same run-ID scheme, so a shared tree means v0.1.0's r1-agentlens-M07
# is indistinguishable from v0.2.0's -- plan_runs would read the old answer,
# call the run finished and skip it, and the new campaign would quietly inherit
# 108 runs produced by a different tool version and a different worker model.
# Point BENCH_RUNS_ROOT at runs_v1 to re-grade or audit the frozen campaign.
RUNS_ROOT = Path(os.environ.get("BENCH_RUNS_ROOT", REPO_ROOT / ".eval" / "runs_v2")).resolve()
RUNS_ROOT_V1 = REPO_ROOT / ".eval" / "runs_v1"

TASK_COUNT = 18
ARMS = ("agentlens", "baseline", "linerange")

# The schedule was pre-registered for 5 repetitions and is generated from that
# number, so it stays at 5: regenerating it to 3 would rewrite the
# pre-registration to match the outcome, which is the one edit a benchmark
# author must never make.
REPETITIONS_SCHEDULED = 5
# What the campaign actually executed. Cut from 5 to 3 on 2026-07-30 at the
# user's instruction -- "No more repitions this is way too expensive" -- after
# repetition 3 finished and *before* anything was graded. The truncation is a
# cost decision, not a result-driven one; §6 of METHODOLOGY.md records it.
# Repetitions 4 and 5 remain in schedule.json, unexecuted and reported as such.
REPETITIONS = 3
EXPECTED_RUNS = TASK_COUNT * REPETITIONS * len(ARMS)
