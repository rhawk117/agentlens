#!/usr/bin/env python3
"""Adversarial tests for the PreToolUse isolation gate.

The gate is the layer that stops a worker reaching the subject outside the
metering wrapper. A gate that silently stops matching -- because a tool grew an
argument, or because the worker was dispatched with a toolset nobody expected --
fails open: the benchmark still produces numbers, and the numbers are wrong.

So these tests are written as attacks. Each one is a route to the subject or to
gold that a worker could plausibly take.
"""

from __future__ import annotations

import importlib.util
import unittest

from paths import REPO_ROOT

_spec = importlib.util.spec_from_file_location(
    "eval_subject_gate", REPO_ROOT / ".claude" / "hooks" / "eval_subject_gate.py"
)
assert _spec is not None and _spec.loader is not None
gate = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(gate)

SUBJECT = gate.DJANGO_ROOT
WRAPPER_CALL = (
    "/home/rhawk/dev/agentlens/evals/harness/.venv/bin/python "
    "/home/rhawk/dev/agentlens/evals/harness/bench_tool.py r1-agentlens-M07 map ."
)


def verdict(tool_name: str, tool_input: dict) -> str | None:
    return gate.decision(tool_name, gate.requested_text(tool_name, tool_input))


class ShellRoutes(unittest.TestCase):
    def test_reading_the_subject_directly_is_denied(self) -> None:
        for command in (
            f"cat {SUBJECT}/django/core/handlers/base.py",
            f"rg process_template_response {SUBJECT}",
            f"cd {SUBJECT} && ls django/core",
            f"head -n 200 {SUBJECT}/django/utils/deprecation.py",
        ):
            with self.subTest(command=command):
                self.assertIsNotNone(verdict("Bash", {"command": command}))

    def test_the_wrapper_is_allowed(self) -> None:
        self.assertIsNone(verdict("Bash", {"command": WRAPPER_CALL}))

    def test_reading_gold_is_denied(self) -> None:
        for name in ("tasks.json", "gold.sha256", "protocol.json", "schedule.json"):
            with self.subTest(name=name):
                self.assertIsNotNone(verdict("Bash", {"command": f"cat evals/harness/{name}"}))

    def test_ordinary_work_is_untouched(self) -> None:
        # The gate runs in every session, not only during a campaign. If it
        # denied routine commands it would be turned off, and then it protects
        # nothing at all.
        for command in ("cargo test --workspace", "git status", "ls ~/dev"):
            with self.subTest(command=command):
                self.assertIsNone(verdict("Bash", {"command": command}))

    def test_the_override_must_lead_the_command(self) -> None:
        # A worker echoing the escape hatch mid-command must not slip past it.
        self.assertIsNone(
            verdict("Bash", {"command": "BENCH_GATE=off cat evals/harness/tasks.json"})
        )
        self.assertIsNotNone(
            verdict("Bash", {"command": "cat evals/harness/tasks.json # BENCH_GATE=off"})
        )


class FileToolRoutes(unittest.TestCase):
    """The routes that opened up when the restricted agent definition did not load."""

    def test_read_of_the_subject_is_denied(self) -> None:
        self.assertIsNotNone(
            verdict("Read", {"file_path": f"{SUBJECT}/django/core/handlers/base.py"})
        )

    def test_grep_over_the_subject_is_denied(self) -> None:
        # Grep carries the target in `path`, not in the pattern.
        self.assertIsNotNone(
            verdict("Grep", {"pattern": "process_template_response", "path": str(SUBJECT)})
        )

    def test_glob_over_the_subject_is_denied(self) -> None:
        # Glob can carry the whole target in `pattern` with no `path` at all.
        self.assertIsNotNone(verdict("Glob", {"pattern": f"{SUBJECT}/django/**/*.py"}))

    def test_read_of_gold_is_denied(self) -> None:
        self.assertIsNotNone(verdict("Read", {"file_path": "evals/harness/tasks.json"}))

    def test_the_shell_override_does_not_apply_to_file_tools(self) -> None:
        # There is no command line to prefix, so a path that merely contains the
        # token must not be treated as an escape hatch.
        self.assertIsNotNone(verdict("Read", {"file_path": f"{SUBJECT}/BENCH_GATE=off/x.py"}))

    def test_reading_a_run_prompt_is_allowed(self) -> None:
        # The orchestrator inspects rendered prompts; they are not gold.
        self.assertIsNone(
            verdict("Read", {"file_path": ".eval/runs_v2/repetition-1/agentlens/M07/prompt.txt"})
        )


if __name__ == "__main__":
    unittest.main()
