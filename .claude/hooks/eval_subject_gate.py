#!/usr/bin/env python3
"""Deny any shell command that reaches the benchmark subject outside the wrapper.

A benchmark worker holds `Bash`, and `Bash` can read the Django checkout
directly. If it does, the tokens it consumed are never counted, and token cost
is the metric the whole benchmark exists to report -- a single unmetered `cat`
of a large module silently makes an arm look cheaper than it was.

The worker's prompt already forbids this. Prompts are not enforcement, so this
is the second layer, and `detect_leaks.py` is the third: the hook can be
misconfigured, but the post-hoc check reads what actually happened.

Deliberately narrow. It denies commands that name the subject corpus or the
harness's own answer key, and allows everything else, so it cannot brick an
ordinary session. Set BENCH_GATE=off to disable it for one command.
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


def decision(command: str) -> str | None:
    """Return a denial reason, or None to allow."""
    if OVERRIDE.match(command):
        return None
    if WRAPPER.search(command):
        # The wrapper validates its own arguments and meters what it runs.
        return None
    if DJANGO_ROOT and DJANGO_ROOT in command:
        return (
            f"This command names the benchmark subject ({DJANGO_ROOT}) without going "
            "through bench_tool.py. Every observation of the subject must be metered, "
            "or the token cost this benchmark reports is wrong. Use the wrapper "
            "invocation given in your prompt."
        )
    if ANSWER_KEY.search(command):
        return (
            "This command reads the benchmark's answer key or protocol. A run that "
            "sees gold is not measuring navigation. Use only the wrapper."
        )
    return None


def main() -> None:
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return
    command = str(payload.get("tool_input", {}).get("command", ""))
    if not command:
        return
    reason = decision(command)
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
