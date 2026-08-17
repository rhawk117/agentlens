"""test_regrade_of_runs_v2_passes_the_preregistered_thresholds reproduces the
published v0.2.0 outcome: every falsification threshold passes.
"""

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
    with pytest.raises(report.ReportError, match="agentlens must be among"):
        report.grade_campaign(tmp_path, 1, ["baseline"])
