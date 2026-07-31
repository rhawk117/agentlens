#!/usr/bin/env python3
"""Check that every answer rests on evidence its own transcript contains.

This is the authoritative isolation check, and the only one that reads what
actually happened rather than what was supposed to. The worker's prompt forbids
reading the subject directly and the PreToolUse hook denies it, but a prompt
can be ignored and a hook can be misconfigured or disabled; a transcript cannot
be argued with.

The test: every repo-relative source path an answer cites must appear somewhere
in that run's captured tool output. An answer naming a file the run never
retrieved either came from the model's prior knowledge of Django -- which the
benchmark forbids, because then it is measuring memorisation, not navigation --
or from an unmetered read, which corrupts the token cost.

Deliberately conservative. It flags only *cited paths*, not prose, because a
worker can legitimately describe behaviour in words it never saw verbatim. A
flagged run is quarantined and re-run, and the quarantine count is published:
silently dropping runs would bias the sample toward whatever the tool is good at.
"""

from __future__ import annotations

import argparse
import json
import re

from paths import ARMS, ROOT
from plan_runs import run_directory, scheduled_runs

# A repo-relative Python path as answers cite them: `django/core/handlers/base.py`.
CITED_PATH = re.compile(r"\b((?:[\w.-]+/)+[\w.-]+\.py)\b")


def transcript_text(run_id: str) -> str | None:
    path = run_directory(run_id) / "transcript.jsonl"
    if not path.exists():
        return None
    chunks: list[str] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line:
            continue
        record = json.loads(line)
        # Both the arguments and the output count as evidence the run saw: asking
        # `slice x.py#Foo` and being told it does not exist still means the path
        # entered this run legitimately rather than from memory.
        chunks.append(record.get("stdout", ""))
        chunks.append(record.get("stderr", ""))
        chunks.append(" ".join(str(arg) for arg in record.get("args", [])))
    return "\n".join(chunks)


def unsupported_citations(run_id: str) -> list[str] | None:
    """Paths the answer cites that never appear in the transcript, or None."""
    answer_path = run_directory(run_id) / "answer.txt"
    if not answer_path.exists():
        return None
    transcript = transcript_text(run_id)
    if transcript is None:
        return None
    answer = answer_path.read_text(encoding="utf-8")
    cited = {match.group(1) for match in CITED_PATH.finditer(answer.replace("\\", "/"))}
    return sorted(path for path in cited if path not in transcript)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quarantine", action="store_true", help="write quarantine.json")
    options = parser.parse_args()

    quarantined: dict[str, list[str]] = {}
    checked = 0
    for run_id in scheduled_runs():
        leaks = unsupported_citations(run_id)
        if leaks is None:
            continue
        checked += 1
        if leaks:
            quarantined[run_id] = leaks

    print(f"checked {checked} completed runs")
    if not quarantined:
        print("no run cites evidence absent from its own transcript")
        return

    print(f"QUARANTINE {len(quarantined)} run(s):")
    for run_id, paths in sorted(quarantined.items()):
        print(f"  {run_id}: {', '.join(paths)}")
    by_arm = {arm: sum(1 for r in quarantined if r.split("-", 2)[1] == arm) for arm in ARMS}
    print(f"by arm: {by_arm}")

    if options.quarantine:
        (ROOT / "quarantine.json").write_text(
            json.dumps(quarantined, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print("wrote quarantine.json")
    # Non-zero so a driver script stops rather than grading contaminated runs.
    raise SystemExit(1)


if __name__ == "__main__":
    main()
