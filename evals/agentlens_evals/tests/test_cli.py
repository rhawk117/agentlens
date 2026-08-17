"""cli.main is the only boundary that maps domain exceptions to exit codes."""

import sys

import pytest

from agentlens_evals import cli
from agentlens_evals.campaign import CampaignError
from agentlens_evals.report import ReportError


def test_campaign_error_exits_one_with_its_message(monkeypatch, capsys) -> None:
    async def raise_campaign_error(version, runs_root, concurrency):
        raise CampaignError("run order mismatch: expected r1-agentlens-M01, got x")

    monkeypatch.setattr(cli.campaign, "run_campaign", raise_campaign_error)
    monkeypatch.setattr(
        sys, "argv", ["agentlens-evals", "run", "--tool-version", "9.9.9"]
    )

    with pytest.raises(SystemExit) as excinfo:
        cli.main()

    assert excinfo.value.code == 1
    assert "run order mismatch" in capsys.readouterr().err


def test_report_error_exits_one_with_its_message(monkeypatch, capsys) -> None:
    def raise_report_error(runs_root, repetitions, arms):
        raise ReportError("agentlens must be among the graded arms")

    monkeypatch.setattr(cli.report, "grade_campaign", raise_report_error)
    monkeypatch.setattr(sys, "argv", ["agentlens-evals", "grade"])

    with pytest.raises(SystemExit) as excinfo:
        cli.main()

    assert excinfo.value.code == 1
    assert "agentlens must be among" in capsys.readouterr().err
