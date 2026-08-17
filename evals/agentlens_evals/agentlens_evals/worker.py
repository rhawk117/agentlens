"""One benchmark session: a Haiku 4.5 worker that may touch only the wrapper.

The prompt semantics are ported from attest.py:render_prompt; the isolation
that eval_subject_gate.py provided for interactive dispatch is provided here
in two layers, both driven by the same decision (``_tool_call_permitted``):
a PreToolUse hook, which runs before the CLI's own "safe command"
auto-approval and so is the layer that actually stops it, and the
``can_use_tool`` permission callback, kept as defense in depth for whatever
the hook doesn't intercept. Bash is the only tool, and the only Bash command
either layer permits is this run's own metering-wrapper invocation. The
CLI's built-in "safe command" auto-approval let commands like `echo` or
`cat` through `can_use_tool` unchallenged, observed empirically in a live
smoke session; the PreToolUse hook fires before that heuristic, so it is the
layer that must carry enforcement.

``_tool_call_permitted`` is the one decision both layers enforce, fail-closed:
any input shape other than ``Bash`` with a string ``command`` field is denied
without inspection.

WORKER_MODEL is pinned by decision, not configuration: cost (a prior
campaign was cut from 5 to 3 repetitions over expense) and comparability
(v1/v2 attestations record this model). Dispatch refuses anything else.

UNQUOTED_METACHARACTERS matches shell metacharacters outside single quotes,
which would turn one wrapper call into two commands; quoted answers may
contain anything.

worker_options omits Read/Grep/Glob/Write/Edit/etc. from ``tools`` entirely
(not merely unapproved), sets ``allowed_tools=[]`` so nothing is
auto-approved and every call reaches the callback, registers the PreToolUse
hook with no matcher so it catches every tool call rather than only Bash
(the hook itself denies anything that isn't Bash), and sets
``setting_sources=[]`` so no CLAUDE.md, settings-file hooks, or user
settings apply.

dispatch relies on ``ClaudeSDKClient.connect()`` with no prompt holding the
control channel open for the life of the ``async with`` block: it never
spawns the SDK's ``stream_input()`` closing task (that only happens for a
non-None prompt -- see ``claude_agent_sdk.client.ClaudeSDKClient._connect_inner``),
so stdin stays open across the whole permission round-trip regardless of the
sdk_mcp_servers/hooks-gated closing logic ``query()``'s one-shot prompt
generator relies on instead (that gate is what failed a prior campaign, back
when no hooks were registered).
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any

from claude_agent_sdk import (
    ClaudeAgentOptions,
    ClaudeSDKClient,
    HookMatcher,
    PermissionResultAllow,
    PermissionResultDeny,
    ResultMessage,
)

from agentlens_evals.paths import DJANGO_ROOT, PROJECT_ROOT, REPO_ROOT, RunId

WORKER_MODEL = "claude-haiku-4-5-20251001"


class WorkerModelError(RuntimeError):
    pass


class DispatchError(RuntimeError):
    pass


ARM_INTERFACE = {
    "agentlens": (
        "Available research commands: slice, map, find, literals, callers, packet, and dead.\n"
    ),
    "baseline": (
        "Available research commands: rg matches and cat whole-file reads. "
        "Line-range and context reads are unavailable.\n"
    ),
    "linerange": (
        "Available research commands: rg matches and sed line-range reads, "
        "written exactly as `sed -n 'START,ENDp' FILE`. Whole-file reads are "
        "unavailable: read only the line ranges you need.\n"
    ),
}

UNQUOTED_METACHARACTERS = re.compile(r"[;&|<>`\n]|\$\(")


def wrapper_invocation(run_id: str) -> str:
    return (
        f"uv run --project {PROJECT_ROOT} python -m agentlens_evals.metering {run_id}"
    )


def render_prompt(run_id: str, task_prompt: str) -> str:
    parsed = RunId.parse(run_id)
    arm, task_id = parsed.arm, parsed.task_id
    common = (
        "You are one independent benchmark worker answering one question about the "
        "pinned Django 6.0.7 source checkout. Use only evidence retrieved during "
        "this run. Do not inspect the rubric, gold files, benchmark implementation, "
        "or another run.\n\n"
        "Do not use direct file reads, shell search commands, web access, prior "
        "knowledge, or any other tool to inspect the subject. You may invoke only "
        "the benchmark wrapper through the shell. Stop researching when it reports "
        "a cap.\n\n"
        "For comprehension tasks, answer concisely with the behavior and "
        "repo-relative source address(es). For localization tasks, return only the "
        "repo-relative address(es) you would edit.\n\n"
    )
    command = (
        f"\nInvoke research as:\n{wrapper_invocation(run_id)} COMMAND ARGS...\n\n"
        "After composing the answer, submit exactly once as:\n"
        f"{wrapper_invocation(run_id)} submit 'COMPLETE ANSWER'\n\n"
        "Then return only `submitted` as your final response.\n\n"
        f"Task {task_id}: {task_prompt}\n"
    )
    return common + ARM_INTERFACE[arm] + command


def command_permitted(command: str, run_id: str) -> bool:
    stripped = command.strip()
    prefix = wrapper_invocation(run_id)
    if not (stripped == prefix or stripped.startswith(prefix + " ")):
        return False
    unquoted = re.sub(r"'[^']*'", "", stripped)
    return not UNQUOTED_METACHARACTERS.search(unquoted)


DENY_REASON = "only this run's metering-wrapper invocation is permitted"


def _tool_call_permitted(tool_name: Any, tool_input: Any, run_id: str) -> bool:
    if tool_name != "Bash" or not isinstance(tool_input, dict):
        return False
    command = tool_input.get("command")
    if not isinstance(command, str):
        return False
    return command_permitted(command, run_id)


def permission_callback(run_id: str):
    async def can_use_tool(tool_name: str, input_data: dict, context):
        if _tool_call_permitted(tool_name, input_data, run_id):
            return PermissionResultAllow()
        return PermissionResultDeny(message=DENY_REASON, interrupt=False)

    return can_use_tool


def pretooluse_hook(run_id: str):
    async def hook(input_data: dict, tool_use_id: str | None, context: dict):
        tool_name = input_data.get("tool_name")
        tool_input = input_data.get("tool_input")
        if _tool_call_permitted(tool_name, tool_input, run_id):
            decision = "allow"
            reason = None
        else:
            decision = "deny"
            reason = DENY_REASON
        hook_specific_output: dict[str, Any] = {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
        }
        if reason is not None:
            hook_specific_output["permissionDecisionReason"] = reason
        return {"hookSpecificOutput": hook_specific_output}

    return hook


def worker_options(run_id: str, runs_root: Path, binary: Path) -> ClaudeAgentOptions:
    return ClaudeAgentOptions(
        model=WORKER_MODEL,
        tools=["Bash"],
        allowed_tools=[],
        can_use_tool=permission_callback(run_id),
        hooks={"PreToolUse": [HookMatcher(hooks=[pretooluse_hook(run_id)])]},
        permission_mode="default",
        cwd=str(REPO_ROOT),
        setting_sources=[],
        max_turns=60,
        env={
            "BENCH_RUNS_ROOT": str(runs_root),
            "BENCH_AGENTLENS": str(binary),
            "BENCH_DJANGO_ROOT": str(DJANGO_ROOT),
            "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1",
        },
    )


async def dispatch(run_id: str, prompt: str, options: ClaudeAgentOptions) -> str:
    if options.model != WORKER_MODEL:
        raise WorkerModelError(
            f"worker model must be {WORKER_MODEL}, got {options.model!r}"
        )
    async with ClaudeSDKClient(options=options) as client:
        await client.query(prompt)
        async for message in client.receive_response():
            if isinstance(message, ResultMessage):
                if message.subtype == "success":
                    return message.result
                raise DispatchError(
                    f"run {run_id} ended with non-success result: {message.subtype}"
                )
    raise DispatchError(f"run {run_id} produced no result message")
