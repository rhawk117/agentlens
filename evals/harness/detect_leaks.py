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


def classify(run_id: str) -> str | None:
    """ "clean", "memorisation", "breach", or None if the run has no answer.

    The two flagged classes need different remedies, and collapsing them gets
    one of the two wrong.

    A run that still had budget and cited a file it never retrieved is a
    **breach**: something reached the subject outside the wrapper, or the model
    answered from its own memory of Django while it could still have looked.
    That contaminates the token cost, so the run is quarantined and re-run.

    A **capped** run is a different animal. The harness stops it and its own
    prompt tells it to submit the best answer available, so an unretrieved
    citation is the cap talking rather than a bypass -- and with the gate in
    place there was no unmetered read available to it. Re-running it would not
    remove the memorisation; it would just re-roll a hard task until the answer
    changed. That is reported, not re-rolled.
    """
    leaks = unsupported_citations(run_id)
    if leaks is None:
        return None
    if not leaks:
        return "clean"
    return "memorisation" if (run_directory(run_id) / "capped").exists() else "breach"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quarantine", action="store_true", help="write quarantine.json")
    options = parser.parse_args()

    flagged: dict[str, dict[str, object]] = {}
    checked = 0
    for run_id in scheduled_runs():
        verdict = classify(run_id)
        if verdict is None:
            continue
        checked += 1
        if verdict != "clean":
            flagged[run_id] = {"class": verdict, "paths": unsupported_citations(run_id)}

    breaches = {r: d for r, d in flagged.items() if d["class"] == "breach"}
    memorised = {r: d for r, d in flagged.items() if d["class"] == "memorisation"}

    print(f"checked {checked} completed runs")
    for label, group in (("BREACH", breaches), ("MEMORISATION (capped)", memorised)):
        if not group:
            continue
        print(f"{label}: {len(group)} run(s)")
        for run_id, detail in sorted(group.items()):
            print(f"  {run_id}: {', '.join(detail['paths'])}")
        by_arm = {arm: sum(1 for r in group if r.split("-", 2)[1] == arm) for arm in ARMS}
        print(f"  by arm: {by_arm}")

    if not flagged:
        print("no run cites evidence absent from its own transcript")

    if options.quarantine:
        (ROOT / "quarantine.json").write_text(
            json.dumps(flagged, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
        print("wrote quarantine.json")

    # Only a breach stops the driver. Capped memorisation is published alongside
    # the results instead, because re-running it is indistinguishable from
    # re-rolling until the tool under test looks better.
    if breaches:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
