from collections.abc import AsyncIterable

import pytest

from agentlens_evals import worker

RUN = "r1-linerange-M03"


def test_prompt_carries_isolation_interface_and_submit_contract() -> None:
    prompt = worker.render_prompt(RUN, "What does BaseHandler do?")
    assert "one independent benchmark worker" in prompt
    assert "sed line-range reads" in prompt  # linerange arm interface
    assert worker.wrapper_invocation(RUN) in prompt
    assert "submit exactly once" in prompt
    assert "Task M03: What does BaseHandler do?" in prompt


def test_each_arm_gets_its_own_interface() -> None:
    assert "slice, map, find" in worker.render_prompt("r1-agentlens-M03", "q")
    assert "cat whole-file reads" in worker.render_prompt("r1-baseline-M03", "q")


def test_command_permitted_only_for_the_wrapper() -> None:
    good = worker.wrapper_invocation(RUN) + " rg get_response django"
    assert worker.command_permitted(good, RUN)
    submit = worker.wrapper_invocation(RUN) + " submit 'found; it & works | fine'"
    assert worker.command_permitted(submit, RUN)  # metacharacters inside quotes
    assert not worker.command_permitted("cat /etc/passwd", RUN)
    assert not worker.command_permitted(good + " && cat /etc/passwd", RUN)
    assert not worker.command_permitted(good + " ; rm -rf /", RUN)
    assert not worker.command_permitted(good + " > /tmp/exfil", RUN)
    assert not worker.command_permitted(good + " $(cat gold)", RUN)
    other_run = worker.wrapper_invocation("r1-baseline-M01") + " cat x.py"
    assert not worker.command_permitted(other_run, RUN)


async def test_permission_callback_denies_non_bash_tools() -> None:
    can_use = worker.permission_callback(RUN)
    verdict = await can_use("Read", {"file_path": "/etc/passwd"}, None)
    assert verdict.behavior == "deny"
    verdict = await can_use("Bash", {"command": "cat /etc/passwd"}, None)
    assert verdict.behavior == "deny"
    verdict = await can_use(
        "Bash", {"command": worker.wrapper_invocation(RUN) + " map ."}, None
    )
    assert verdict.behavior == "allow"


async def test_dispatch_refuses_any_model_but_haiku(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    options.model = "claude-fable-5"
    with pytest.raises(worker.WorkerModelError):
        await worker.dispatch(RUN, "prompt", options)


async def test_dispatch_sends_streaming_prompt_when_can_use_tool_is_set(
    tmp_path, monkeypatch
) -> None:
    # The SDK's can_use_tool callback requires the prompt to arrive as an
    # AsyncIterable of message dicts, not a plain str -- see
    # claude_agent_sdk._internal.client._process_query_inner.
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    captured: dict[str, object] = {}

    async def fake_query(*, prompt, options):
        captured["prompt"] = prompt
        return
        yield  # pragma: no cover - makes this an async generator

    monkeypatch.setattr(worker, "query", fake_query)

    await worker.dispatch(RUN, "prompt text", options)

    assert isinstance(captured["prompt"], AsyncIterable)
    assert not isinstance(captured["prompt"], str)


def test_worker_options_are_hermetic(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    assert options.model == worker.WORKER_MODEL
    assert options.setting_sources == []
    assert options.allowed_tools == []
    assert options.env["BENCH_RUNS_ROOT"] == str(tmp_path)
    assert options.env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] == "1"
