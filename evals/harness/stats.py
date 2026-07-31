"""Campaign statistics the results document needs but the grader does not record.

Run behaviour -- how many calls an arm spent, how many of them failed, and why
-- lives in the transcripts rather than in results.json. The failure taxonomy is
the point: v0.1.0 reported that agentlens burns calls on address-syntax errors
without ever quantifying it against the controls.
"""

from __future__ import annotations

import json
import re
import statistics
from collections import Counter, defaultdict

from paths import ARMS
from plan_runs import run_directory, scheduled_runs

# A nonzero exit is not automatically the tool's fault, and reporting one raw
# "failure rate" per arm would hide the only comparison worth making.
#
# `rg` exits 1 when it matches nothing, and so does `agentlens find` -- in both
# cases the arm asked a well-formed question and got a true, useful answer:
# there is nothing there. That is navigation, not a defect.
#
# A *usage* failure is different. The arm did not know how to phrase the call,
# so the call bought nothing and the budget still paid for it. Separating the
# two is what makes agentlens's rate comparable to the controls' at all.
# `no symbol X in Y` belongs on the empty-result side even though it names a
# thing that does not exist: the call was well formed and the honest answer is
# that the target is absent, which is exactly what `rg` exiting 1 means. Ruling
# it a usage error would inflate agentlens's error rate against controls whose
# equivalent outcome is counted as a legitimate search.
EMPTY_RESULT = re.compile(
    r"\bno (match|matches|literal|literals|caller|callers|result|results"
    r"|dead|call|calls|symbol|symbols|occurrence|occurrences|definition|definitions)\b",
    flags=re.IGNORECASE,
)


def failure_class(record: dict) -> str:
    combined = record.get("stderr", "") + record.get("stdout", "")
    if EMPTY_RESULT.search(combined):
        return "empty result"
    if not combined.strip():
        # rg says nothing at all when it matches nothing.
        return "empty result"
    return "usage error"


def usage_detail(record: dict) -> str:
    combined = (record.get("stderr", "") + record.get("stdout", "")).strip()
    lines = combined.splitlines()
    return lines[0][:70] if lines else f"exit {record['exit_code']}"


def main() -> None:
    calls: dict[str, list[int]] = defaultdict(list)
    tokens: dict[str, list[int]] = defaultdict(list)
    capped: dict[str, int] = defaultdict(int)
    total: dict[str, int] = defaultdict(int)
    classes: dict[str, Counter] = defaultdict(Counter)
    details: dict[str, Counter] = defaultdict(Counter)

    for run_id in scheduled_runs():
        arm = run_id.split("-", 2)[1]
        directory = run_directory(run_id)
        records = [
            json.loads(line)
            for line in (directory / "transcript.jsonl").read_text().splitlines()
            if line
        ]
        calls[arm].append(len(records))
        tokens[arm].append(sum(int(record["result_tokens"]) for record in records))
        capped[arm] += (directory / "capped").exists()
        total[arm] += len(records)
        for record in records:
            if int(record["exit_code"]) != 0:
                failure = failure_class(record)
                classes[arm][failure] += 1
                if failure == "usage error":
                    details[arm][usage_detail(record)] += 1

    for arm in ARMS:
        usage = classes[arm]["usage error"]
        empty = classes[arm]["empty result"]
        print(
            f"{arm:10s} calls/run median {statistics.median(calls[arm]):4.1f}  "
            f"tokens/run median {statistics.median(tokens[arm]):7.0f}  "
            f"tokens total {sum(tokens[arm]):7d}  "
            f"capped {capped[arm]}/54"
        )
        print(
            f"           of {total[arm]} calls: "
            f"usage errors {usage} ({100 * usage / total[arm]:.1f}%), "
            f"empty results {empty} ({100 * empty / total[arm]:.1f}%)"
        )
        for detail, count in details[arm].most_common(6):
            print(f"    {count:4d}  {detail}")


if __name__ == "__main__":
    main()
