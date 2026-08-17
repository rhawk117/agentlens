"""test_regrade_of_runs_v2_passes_the_preregistered_thresholds reproduces the
published v0.2.0 outcome: every falsification threshold passes.
"""

import pytest

from agentlens_evals import paths, report

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
        f"missing {' and '.join(missing)}: the falsification threshold "
        "comparisons in grade_campaign are unverified without the archived corpus"
    )


_CORPUS_SKIP_REASON = _missing_corpus_reason()
needs_corpus = pytest.mark.skipif(
    _CORPUS_SKIP_REASON is not None,
    reason=_CORPUS_SKIP_REASON or "",
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
