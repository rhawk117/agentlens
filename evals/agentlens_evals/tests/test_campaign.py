import json
from pathlib import Path

import pytest

from agentlens_evals import campaign, paths, worker


def test_schedule_matches_the_frozen_harness_order() -> None:
    runs = campaign.scheduled_runs()
    assert len(runs) == paths.EXPECTED_RUNS
    assert len(set(runs)) == paths.EXPECTED_RUNS
    schedule = json.loads(
        (paths.HARNESS_ROOT / "schedule.json").read_text(encoding="utf-8")
    )["schedule"]
    first = schedule[0]
    assert runs[0] == f"r1-{first['arm_order'][0]}-{first['task_order'][0]}"
    assert all(run.startswith(("r1-", "r2-", "r3-")) for run in runs)


def seed_run(
    runs_root: Path,
    run_id: str,
    *,
    answer: str | None,
    transcript: list[dict] | None,
    capped: bool = False,
) -> Path:
    directory = campaign.run_directory(run_id, runs_root)
    directory.mkdir(parents=True, exist_ok=True)
    if transcript is not None:
        with (directory / "transcript.jsonl").open("w") as stream:
            for record in transcript:
                stream.write(json.dumps(record) + "\n")
    if answer is not None:
        (directory / "answer.txt").write_text(answer + "\n", encoding="utf-8")
    if capped:
        (directory / "capped").touch()
    return directory


def test_status_counts_complete_stranded_pending(tmp_path) -> None:
    runs = campaign.scheduled_runs()
    seed_run(tmp_path, runs[0], answer="done", transcript=[{"result_tokens": 1}])
    seed_run(tmp_path, runs[1], answer=None, transcript=[{"result_tokens": 1}])
    status = campaign.campaign_status(tmp_path)
    assert status["complete"] == 1
    assert status["stranded"] == [runs[1]]
    assert status["pending"] == paths.EXPECTED_RUNS - 1
    assert status["total"] == paths.EXPECTED_RUNS


def test_attestation_start_order_is_enforced(tmp_path) -> None:
    runs = campaign.scheduled_runs()
    campaign.record_start(tmp_path, runs[0], "prompt zero")
    with pytest.raises(campaign.CampaignError, match="run order mismatch"):
        campaign.record_start(tmp_path, runs[5], "out of order")
    campaign.record_start(tmp_path, runs[1], "prompt one")
    events = campaign.read_events(tmp_path)
    assert [event["run_id"] for event in events] == runs[:2]
    assert all(event["model"] == "claude-haiku-4-5-20251001" for event in events)


def test_leak_classification(tmp_path) -> None:
    runs = campaign.scheduled_runs()
    transcript = [
        {"stdout": "django/core/handlers/base.py:1:x", "stderr": "", "args": []}
    ]
    seed_run(
        tmp_path,
        runs[0],
        answer="see django/core/handlers/base.py",
        transcript=transcript,
    )
    assert campaign.classify(runs[0], tmp_path) == "clean"
    seed_run(
        tmp_path, runs[1], answer="see django/urls/resolvers.py", transcript=transcript
    )
    assert campaign.classify(runs[1], tmp_path) == "breach"
    seed_run(
        tmp_path,
        runs[2],
        answer="see django/urls/resolvers.py",
        transcript=transcript,
        capped=True,
    )
    assert campaign.classify(runs[2], tmp_path) == "memorisation"
    seed_run(tmp_path, runs[3], answer=None, transcript=transcript)
    assert campaign.classify(runs[3], tmp_path) is None


async def test_run_campaign_dispatches_only_outstanding(tmp_path, monkeypatch) -> None:
    runs = campaign.scheduled_runs()
    for run_id in runs[2:]:
        seed_run(
            tmp_path,
            run_id,
            answer="already done",
            transcript=[{"stdout": "", "stderr": "", "args": []}],
        )
    monkeypatch.setattr(
        campaign.subject, "verify", lambda version: Path("/fake/agentlens")
    )

    dispatched: list[str] = []

    async def fake_dispatch(run_id, prompt, options):
        dispatched.append(run_id)
        seed_run(
            tmp_path,
            run_id,
            answer="worker answer",
            transcript=[{"stdout": "", "stderr": "", "args": []}],
        )
        return "submitted"

    monkeypatch.setattr(campaign.worker, "dispatch", fake_dispatch)
    summary = await campaign.run_campaign("9.9.9", tmp_path, concurrency=2)
    assert sorted(dispatched) == sorted(runs[:2])
    assert summary["dispatched"] == 2
    assert summary["breaches"] == []


async def test_run_campaign_counts_a_failed_dispatch_without_aborting(
    tmp_path, monkeypatch
) -> None:
    runs = campaign.scheduled_runs()
    for run_id in runs[2:]:
        seed_run(
            tmp_path,
            run_id,
            answer="already done",
            transcript=[{"stdout": "", "stderr": "", "args": []}],
        )
    monkeypatch.setattr(
        campaign.subject, "verify", lambda version: Path("/fake/agentlens")
    )

    failing_run = runs[0]
    succeeding_run = runs[1]

    async def fake_dispatch(run_id, prompt, options):
        if run_id == failing_run:
            raise worker.DispatchError(f"run {run_id} ended with non-success result")
        seed_run(
            tmp_path,
            run_id,
            answer="worker answer",
            transcript=[{"stdout": "", "stderr": "", "args": []}],
        )
        return "submitted"

    monkeypatch.setattr(campaign.worker, "dispatch", fake_dispatch)
    summary = await campaign.run_campaign("9.9.9", tmp_path, concurrency=2)
    assert summary["failed_dispatches"] == 1
    assert campaign.is_complete(succeeding_run, tmp_path)
    assert not campaign.is_complete(failing_run, tmp_path)
