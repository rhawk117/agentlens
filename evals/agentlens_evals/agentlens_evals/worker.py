"""One benchmark session: a Haiku 4.5 worker that may touch only the wrapper.

The prompt semantics are ported from attest.py:render_prompt; the isolation
that eval_subject_gate.py provided for interactive dispatch is provided here
by the SDK permission callback -- Bash is the only tool, and the only Bash
command permitted is this run's own metering-wrapper invocation.
"""

from __future__ import annotations

import re
from pathlib import Path

from claude_agent_sdk import (
    ClaudeAgentOptions,
    PermissionResultAllow,
    PermissionResultDeny,
    ResultMessage,
    query,
)

from agentlens_evals.paths import DJANGO_ROOT, PROJECT_ROOT, REPO_ROOT

# Pinned by decision, not configuration: cost (a prior campaign was cut from 5
# to 3 repetitions over expense) and comparability (v1/v2 attestations record
# this model). Dispatch refuses anything else.
WORKER_MODEL = "claude-haiku-4-5-20251001"


class WorkerModelError(RuntimeError):
    """Dispatch was configured with a model other than the pinned worker model."""


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

# Shell metacharacters outside single quotes turn one wrapper call into two
# commands. Quoted answers may contain anything.
UNQUOTED_METACHARACTERS = re.compile(r"[;&|<>`\n]|\$\(")


def wrapper_invocation(run_id: str) -> str:
    return (
        f"uv run --project {PROJECT_ROOT} python -m agentlens_evals.metering {run_id}"
    )


def render_prompt(run_id: str, task_prompt: str) -> str:
    _repetition, arm, task_id = run_id.split("-", 2)
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


def permission_callback(run_id: str):
    async def can_use_tool(tool_name: str, input_data: dict, context):
        if tool_name == "Bash" and command_permitted(
            str(input_data.get("command", "")), run_id
        ):
            return PermissionResultAllow()
        return PermissionResultDeny(
            message="only this run's metering-wrapper invocation is permitted",
            interrupt=False,
        )

    return can_use_tool


def worker_options(run_id: str, runs_root: Path, binary: Path) -> ClaudeAgentOptions:
    return ClaudeAgentOptions(
        model=WORKER_MODEL,
        allowed_tools=[],  # nothing auto-approved: every call reaches the callback
        can_use_tool=permission_callback(run_id),
        permission_mode="default",
        cwd=str(REPO_ROOT),
        setting_sources=[],  # no CLAUDE.md, no hooks, no user settings
        max_turns=60,
        env={
            "BENCH_RUNS_ROOT": str(runs_root),
            "BENCH_AGENTLENS": str(binary),
            "BENCH_DJANGO_ROOT": str(DJANGO_ROOT),
            "CLAUDE_CODE_DISABLE_AUTO_MEMORY": "1",
        },
    )


async def dispatch(run_id: str, prompt: str, options: ClaudeAgentOptions) -> str | None:
    if options.model != WORKER_MODEL:
        raise WorkerModelError(
            f"worker model must be {WORKER_MODEL}, got {options.model!r}"
        )
    async for message in query(prompt=prompt, options=options):
        if isinstance(message, ResultMessage):
            return message.result if message.subtype == "success" else None
    return None
