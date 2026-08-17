"""Worker isolation tests.

test_pretooluse_hook_denies_safe_command_heuristic_targets exercises exactly
the commands the CLI's built-in "safe command" auto-approval let through
can_use_tool unchallenged in a live smoke session; the hook is the layer
meant to catch them instead.

_FakeClient stands in for ClaudeSDKClient: connect() with no prompt is what
keeps the SDK's control channel open for the whole permission round-trip
(see worker.dispatch's module docstring) -- it asserts dispatch relies on
that shape rather than the one-shot query() function's prompt generator.
"""

import pytest

from agentlens_evals import worker

RUN = "r1-linerange-M03"


def test_prompt_carries_isolation_interface_and_submit_contract() -> None:
    prompt = worker.render_prompt(RUN, "What does BaseHandler do?")
    assert "one independent benchmark worker" in prompt
    assert "sed line-range reads" in prompt
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
    assert worker.command_permitted(submit, RUN)
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


async def test_pretooluse_hook_denies_safe_command_heuristic_targets() -> None:
    hook = worker.pretooluse_hook(RUN)

    verdict = await hook(
        {"tool_name": "Bash", "tool_input": {"command": "cat foo"}}, "id", {}
    )
    assert verdict["hookSpecificOutput"]["permissionDecision"] == "deny"

    verdict = await hook(
        {"tool_name": "Bash", "tool_input": {"command": "echo hi"}}, "id", {}
    )
    assert verdict["hookSpecificOutput"]["permissionDecision"] == "deny"


async def test_pretooluse_hook_denies_non_bash_tools() -> None:
    hook = worker.pretooluse_hook(RUN)

    verdict = await hook(
        {"tool_name": "Read", "tool_input": {"file_path": "/etc/passwd"}}, "id", {}
    )
    assert verdict["hookSpecificOutput"]["permissionDecision"] == "deny"


async def test_pretooluse_hook_allows_the_exact_wrapper_invocation() -> None:
    hook = worker.pretooluse_hook(RUN)

    verdict = await hook(
        {
            "tool_name": "Bash",
            "tool_input": {"command": worker.wrapper_invocation(RUN) + " map ."},
        },
        "id",
        {},
    )
    assert verdict["hookSpecificOutput"]["permissionDecision"] == "allow"


async def test_pretooluse_hook_fails_closed_on_malformed_input() -> None:
    hook = worker.pretooluse_hook(RUN)

    for malformed in (
        {"tool_name": "Bash"},
        {"tool_name": "Bash", "tool_input": "not a dict"},
        {"tool_name": "Bash", "tool_input": {"command": None}},
        {},
    ):
        verdict = await hook(malformed, "id", {})
        assert verdict["hookSpecificOutput"]["permissionDecision"] == "deny"


async def test_dispatch_refuses_any_model_but_haiku(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    options.model = "claude-fable-5"
    with pytest.raises(worker.WorkerModelError):
        await worker.dispatch(RUN, "prompt", options)


class _FakeClient:
    def __init__(self, calls: dict[str, object], messages: list[object], *, options):
        self._calls = calls
        self._messages = messages
        self.options = options

    async def __aenter__(self):
        self._calls["connect_prompt"] = "unset"
        return self

    async def __aexit__(self, *exc_info):
        return False

    async def query(self, prompt):
        self._calls["query_prompt"] = prompt

    async def receive_response(self):
        for message in self._messages:
            yield message


async def test_dispatch_holds_the_client_open_and_sends_prompt_via_query(
    tmp_path, monkeypatch
) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    calls: dict[str, object] = {}
    result_message = worker.ResultMessage(
        subtype="success",
        duration_ms=1,
        duration_api_ms=1,
        is_error=False,
        num_turns=1,
        session_id="s",
        result="submitted",
    )

    def fake_client(*, options):
        return _FakeClient(calls, [result_message], options=options)

    monkeypatch.setattr(worker, "ClaudeSDKClient", fake_client)

    result = await worker.dispatch(RUN, "prompt text", options)

    assert calls["connect_prompt"] == "unset"
    assert calls["query_prompt"] == "prompt text"
    assert result == "submitted"


def test_worker_options_are_hermetic(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    assert options.model == worker.WORKER_MODEL
    assert options.setting_sources == []
    assert options.tools == ["Bash"]
    assert options.allowed_tools == []
    assert list(options.hooks.keys()) == ["PreToolUse"]
    assert len(options.hooks["PreToolUse"][0].hooks) == 1
    assert options.env["BENCH_RUNS_ROOT"] == str(tmp_path)
    assert options.env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] == "1"
