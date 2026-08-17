"""Console entry point: install / run / status / grade / report."""

from __future__ import annotations

import argparse
import asyncio
import sys
from pathlib import Path

from agentlens_evals import campaign, report, subject
from agentlens_evals.campaign import CampaignError
from agentlens_evals.paths import ARMS, REPETITIONS, RUNS_ROOT
from agentlens_evals.report import ReportError


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

    rep = commands.add_parser(
        "report", help="markdown + derived pydantic-evals reports"
    )
    rep.add_argument("--runs-root", type=Path, default=RUNS_ROOT)
    rep.add_argument("--repetitions", type=int, default=REPETITIONS)
    rep.add_argument("--arms", nargs="+", default=list(ARMS))

    options = parser.parse_args()

    try:
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
    except (CampaignError, ReportError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
