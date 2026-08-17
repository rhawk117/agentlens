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

RunId is the one source of truth for the `r<repetition>-<arm>-<task>` triple
and the run-directory layout it maps to. RunId.parse is the validated
constructor, fail-closed on the same shape metering.py enforced ad hoc;
plain construction is for call sites that already hold validated fields
(metering.py's own gate, report.py's loop variables) and skips
re-validation deliberately.
"""

from __future__ import annotations

import os
from dataclasses import dataclass
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


class InvalidRunId(RuntimeError):
    pass


@dataclass(frozen=True, slots=True)
class RunId:
    repetition: int
    arm: str
    task_id: str

    @classmethod
    def parse(cls, raw: str) -> RunId:
        parts = raw.split("-")
        if len(parts) != 3 or not parts[0].startswith("r"):
            raise InvalidRunId(
                f"run id must be r<1-{REPETITIONS}>-<{'|'.join(ARMS)}>-<task>: {raw!r}"
            )
        try:
            repetition = int(parts[0][1:])
        except ValueError:
            raise InvalidRunId(f"invalid repetition: {raw!r}") from None
        arm, task_id = parts[1], parts[2]
        if repetition not in range(1, REPETITIONS + 1):
            raise InvalidRunId(f"repetition must be 1 through {REPETITIONS}: {raw!r}")
        if arm not in ARMS:
            raise InvalidRunId(f"invalid arm: {arm!r} is not one of {', '.join(ARMS)}")
        if not (
            len(task_id) == 3 and task_id[0] in {"M", "L"} and task_id[1:].isdigit()
        ):
            raise InvalidRunId(f"invalid task id: {raw!r}")
        return cls(repetition, arm, task_id)

    def __str__(self) -> str:
        return f"r{self.repetition}-{self.arm}-{self.task_id}"

    def directory(self, runs_root: Path) -> Path:
        return runs_root / f"repetition-{self.repetition}" / self.arm / self.task_id
