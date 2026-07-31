#!/usr/bin/env python3
"""Deny any shell command that reaches the benchmark subject outside the wrapper.

A benchmark worker can reach the Django checkout through `Bash`, and also
through `Read`, `Grep`, and `Glob` when it runs as a general-purpose agent. If
it does, the tokens it consumed are never counted, and token cost is the metric
the whole benchmark exists to report -- a single unmetered `cat` of a large
module silently makes an arm look cheaper than it was.

Gating only `Bash` was not enough. A custom agent definition restricting a
worker to `Bash` alone is registered when a session starts, so a worker
dispatched in the session that *wrote* that definition still carries the full
toolset. The gate has to hold for whatever tools the worker actually has.

The worker's prompt already forbids this. Prompts are not enforcement, so this
is the second layer, and `detect_leaks.py` is the third: the hook can be
misconfigured, but the post-hoc check reads what actually happened.

Deliberately narrow. It denies calls that name the subject corpus or the
harness's own answer key, and allows everything else, so it cannot brick an
ordinary session. Prefixing a shell command with BENCH_GATE=off disables it for
that command; there is no equivalent for the file tools, so an orchestrator
that needs to see an answer-key file reads it through the shell.
"""

from __future__ import annotations

import json
import os
import re
import sys

# Resolved the same way the harness resolves it, so moving the corpus moves the
# gate with it rather than leaving a rule that guards an empty path.
DJANGO_ROOT = os.environ.get(
    "BENCH_DJANGO_ROOT", os.path.expanduser("~/dev/django-6.0.7")
).rstrip("/")

# The wrapper is the only sanctioned way to touch the subject.
WRAPPER = re.compile(r"bench_tool\.py\b")

# Reading any of these is reading the answer key. A worker that sees gold does
# not have to navigate at all, which is the capability under measurement.
ANSWER_KEY = re.compile(
    r"\b(tasks\.json|gold\.sha256|gold\.blake3|results(_v1(_regraded)?)?\.json"
    r"|RUBRIC\.md|protocol\.json|schedule\.json|attestations(_v1)?\.jsonl"
    r"|report_v1\.md|METHODOLOGY\.md|django-golden\.md)\b"
)


# The orchestrator's own escape hatch. It has to be read out of the command
# text: a hook runs in its own process, so an inline `BENCH_GATE=off cmd` or an
# `export` inside the command never reaches this environment.
OVERRIDE = re.compile(r"^\s*BENCH_GATE=off\b")


def requested_text(tool_name: str, tool_input: dict) -> str:
    """The part of a tool call that can name a file, as one searchable string.

    For `Bash` that is the command. For the file tools it is every string
    argument -- path, pattern and glob alike -- because `Grep` reaches the
    subject through `path` while `Glob` reaches it through `pattern`, and
    guessing which field carries the target per tool is how a gate goes stale
    when a tool gains an argument.
    """
    if tool_name == "Bash":
        return str(tool_input.get("command", ""))
    return "\n".join(value for value in tool_input.values() if isinstance(value, str))


def decision(tool_name: str, request: str) -> str | None:
    """Return a denial reason, or None to allow."""
    # Shell-only: the file tools carry no command line to prefix.
    if tool_name == "Bash" and OVERRIDE.match(request):
        return None
    if tool_name == "Bash" and WRAPPER.search(request):
        # The wrapper validates its own arguments and meters what it runs.
        return None
    if DJANGO_ROOT and DJANGO_ROOT in request:
        return (
            f"This call names the benchmark subject ({DJANGO_ROOT}) without going "
            "through bench_tool.py. Every observation of the subject must be metered, "
            "or the token cost this benchmark reports is wrong. Use the wrapper "
            "invocation given in your prompt."
        )
    if ANSWER_KEY.search(request):
        return (
            "This call reads the benchmark's answer key or protocol. A run that "
            "sees gold is not measuring navigation. Use only the wrapper."
        )
    return None


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return
    tool_name = str(payload.get("tool_name", ""))
    tool_input = payload.get("tool_input", {})
    if not isinstance(tool_input, dict):
        return
    request = requested_text(tool_name, tool_input)
    if not request:
        return
    reason = decision(tool_name, request)
    if reason is None:
        return
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "permissionDecision": "deny",
                    "permissionDecisionReason": reason,
                }
            }
        )
    )


if __name__ == "__main__":
    main()
