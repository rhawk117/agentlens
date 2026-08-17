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


class _FakeClient:
    """Stand-in for ClaudeSDKClient: records connect/query and replays messages.

    connect() with no prompt is what keeps the SDK's control channel open for
    the whole permission round-trip (see worker.dispatch's comment) -- this
    fake asserts dispatch relies on that shape rather than the one-shot
    ``query()`` function's prompt generator.
    """

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

    assert calls["connect_prompt"] == "unset"  # __aenter__ ran: connect() held open
    assert calls["query_prompt"] == "prompt text"
    assert result == "submitted"


def test_worker_options_are_hermetic(tmp_path) -> None:
    options = worker.worker_options(RUN, tmp_path, tmp_path / "agentlens")
    assert options.model == worker.WORKER_MODEL
    assert options.setting_sources == []
    assert options.tools == ["Bash"]
    assert options.allowed_tools == []
    assert options.env["BENCH_RUNS_ROOT"] == str(tmp_path)
    assert options.env["CLAUDE_CODE_DISABLE_AUTO_MEMORY"] == "1"
