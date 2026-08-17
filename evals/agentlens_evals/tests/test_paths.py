"""test_agentlens_binary_has_no_default asserts target/release must never be
consulted implicitly.
"""

from pathlib import Path

import pytest

from agentlens_evals import paths
from agentlens_evals.paths import InvalidRunId, RunId


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
    assert paths.AGENTLENS is None or "target" not in paths.AGENTLENS.parts


def test_run_id_parses_a_well_formed_string() -> None:
    run_id = RunId.parse("r2-baseline-M07")
    assert run_id.repetition == 2
    assert run_id.arm == "baseline"
    assert run_id.task_id == "M07"
    assert str(run_id) == "r2-baseline-M07"


@pytest.mark.parametrize(
    "raw",
    [
        "r2-baseline",
        "r2-baseline-M07-extra",
        "2-baseline-M07",
        "rX-baseline-M07",
        "r0-baseline-M07",
        "r4-baseline-M07",
        "r2-unknown-M07",
        "r2-baseline-M7",
        "r2-baseline-X07",
        "r2-baseline-M0X",
    ],
)
def test_run_id_rejects_malformed_forms(raw: str) -> None:
    with pytest.raises(InvalidRunId):
        RunId.parse(raw)


def test_run_id_directory_matches_the_on_disk_layout() -> None:
    runs_root = Path("/runs")
    run_id = RunId.parse("r3-linerange-L02")
    assert (
        run_id.directory(runs_root) == runs_root / "repetition-3" / "linerange" / "L02"
    )
