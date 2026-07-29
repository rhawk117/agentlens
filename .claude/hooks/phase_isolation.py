#!/usr/bin/env python3
"""PreToolUse hook: block any tool call touching a phase other than the current one.

Fail-closed. Exit 2 blocks the call and returns stderr to the agent.
"""

import json
import os
import re
import sys
from pathlib import Path

# PHASE_LOCK, not LOOP_STATE.md: the lock is written only by advance-phase.sh,
# while LOOP_STATE.md is the agent's own scratchpad. Keying isolation off a file
# the guarded agent may rewrite is not a guard.
# Resolved against CLAUDE_PROJECT_DIR because the hook's cwd is not guaranteed.
STATE = Path(os.environ.get("CLAUDE_PROJECT_DIR", ".")) / ".agent-lens-phase/PHASE_LOCK"
PHASE_TOKEN = re.compile(r"agentlensphase0*(\d+)", re.IGNORECASE)
CURRENT = re.compile(r"^\s*current_phase:\s*(\d+)\s*$", re.IGNORECASE | re.MULTILINE)


def die(message: str) -> None:
    print(f"PHASE_ISOLATION: {message}", file=sys.stderr)
    sys.exit(2)


def current_phase() -> int:
    if not STATE.is_file():
        die(f"{STATE} missing; cannot establish current phase.")
    match = CURRENT.search(STATE.read_text(encoding="utf-8"))
    if not match:
        die(f"{STATE} has no `current_phase: N` line.")
    return int(match.group(1))


def walk(node: object) -> list[str]:
    if isinstance(node, str):
        return [node]
    if isinstance(node, dict):
        return [s for v in node.values() for s in walk(v)]
    if isinstance(node, list):
        return [s for v in node for s in walk(v)]
    return []


def main() -> None:
    payload = json.load(sys.stdin)
    phase = current_phase()
    for text in walk(payload.get("tool_input", {})):
        for hit in PHASE_TOKEN.finditer(text):
            found = int(hit.group(1))
            if found != phase:
                die(
                    f"blocked reference to phase {found} while phase {phase} is active. "
                    f"Later phases are sealed. Do not attempt to read, extract, list, or "
                    f"infer their contents. Finish and merge phase {phase} first."
                )
    sys.exit(0)


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except Exception as exc:
        die(f"hook error, failing closed: {exc}")
