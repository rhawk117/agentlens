"""test_agentlens_binary_has_no_default asserts target/release must never be
consulted implicitly.
"""

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
    assert paths.AGENTLENS is None or "target" not in paths.AGENTLENS.parts
