"""Per-run grading, ported verbatim from the frozen harness grade.py.

Matcher version 2. Any behavioral difference from evals/harness/grade.py on
the runs_v2 corpus is a port bug (verification gate 1) -- do not "improve"
anything here.
"""

from __future__ import annotations

import json
import re
import statistics
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from agentlens_evals.matching import fact_satisfied
from agentlens_evals.paths import DJANGO_ROOT

MATCHER_VERSION = 2
REFERENCE_ARM = "agentlens"
HEDGES = ("might", "possibly", "i'm not sure", "i am not sure", "appears to", "may")

LINE_REFERENCE = re.compile(
    r"\bL(?P<start_l>\d+)(?:\s*-\s*L?(?P<end_l>\d+))?\b"
    r"|:(?P<start_colon>\d+)(?:-(?P<end_colon>\d+))?"
    r"|\blines?\s+(?P<start_word>\d+)(?:\s*(?:-|–|—|to)\s*(?P<end_word>\d+))?\b",
    flags=re.IGNORECASE,
)


def normalize(text: str) -> str:
    return " ".join(text.casefold().split())


def is_negated(text: str, position: int) -> bool:
    prefix = text[max(0, position - 48) : position]
    return bool(
        re.search(
            r"(?:\bnot\b|\bnever\b|\bno\b|\bdoesn't\b|\bdoes not\b|\bisn't\b|\bis not\b)"
            r"(?:\W+\w+){0,4}\W*$",
            prefix,
        )
    )


def contains_asserted(text: str, candidate: str) -> bool:
    for match in re.finditer(re.escape(candidate), text):
        if not is_negated(text, match.start()):
            return True
    return False


def selector_name(address: str) -> str:
    selector = address.split("#", 1)[1]
    if selector.startswith("L"):
        return ""
    return selector.rsplit(".", 1)[-1]


def address_found(answer: str, gold: dict[str, Any]) -> bool:
    normalized = normalize(answer.replace("\\", "/"))
    address = normalize(gold["address"])
    if re.search(rf"{re.escape(address)}(?![\w.])", normalized):
        return True
    path, _selector = gold["address"].split("#", 1)
    path_norm = normalize(path)
    for match in re.finditer(re.escape(path_norm), normalized):
        window = normalized[max(0, match.start() - 100) : match.end() + 100]
        name = normalize(selector_name(gold["address"]))
        if name and re.search(rf"(?<![\w]){re.escape(name)}(?![\w])", window):
            return True
        for line_match in LINE_REFERENCE.finditer(window):
            first = line_match.group("start_l") or line_match.group("start_colon")
            last = line_match.group("end_l") or line_match.group("end_colon")
            if first is None:
                first, last = (
                    line_match.group("start_word"),
                    line_match.group("end_word"),
                )
            start = int(first)
            end = int(last or start)
            if start <= int(gold["end_line"]) and end >= int(gold["start_line"]):
                return True
    return False


def forbidden_penalty(answer: str, claims: list[str]) -> tuple[float, list[str]]:
    normalized = normalize(answer)
    penalty = 0.0
    matched: list[str] = []
    for claim in claims:
        needle = normalize(claim)
        positions = [
            match.start()
            for match in re.finditer(re.escape(needle), normalized)
            if not is_negated(normalized, match.start())
        ]
        if not positions:
            continue
        position = positions[0]
        matched.append(claim)
        window = normalized[max(0, position - 80) : position + len(needle) + 20]
        penalty += 0.1 if any(hedge in window for hedge in HEDGES) else 0.25
    return penalty, matched


def source_lines(gold: dict[str, Any]) -> set[str]:
    path = DJANGO_ROOT / gold["address"].split("#", 1)[0]
    lines = path.read_text(encoding="utf-8").splitlines()
    start = int(gold["start_line"]) - 1
    end = int(gold["end_line"])
    return {line.strip() for line in lines[start:end] if len(line.strip()) >= 8}


def call_retrieves(
    record: dict[str, Any], arm: str, golds: list[dict[str, Any]]
) -> bool:
    if int(record["exit_code"]) not in {0, 1}:
        return False
    output = record["stdout"] + record["stderr"]
    if arm == "agentlens":
        return any(address_found(output, gold) for gold in golds)
    tool = record["tool"]
    args = record["args"]
    if tool == "cat":
        if int(record["exit_code"]) != 0 or not record["stdout"]:
            return False
        requested = {arg.replace("\\", "/") for arg in args}
        return any(gold["address"].split("#", 1)[0] in requested for gold in golds)
    if tool == "sed":
        # Arm C reads a span rather than a file, so retrieval means the span it
        # asked for overlaps the gold span -- not merely that it named the file.
        # Without this, every Arm C run would score as never having found
        # anything and its navigation metric would be uniformly capped.
        if int(record["exit_code"]) != 0 or not record["stdout"] or len(args) != 3:
            return False
        span = re.fullmatch(r"(\d+)(?:,(\d+))?p", args[1])
        if not span:
            return False
        first = int(span.group(1))
        last = int(span.group(2) or first)
        requested_path = args[2].replace("\\", "/")
        return any(
            gold["address"].split("#", 1)[0] == requested_path
            and first <= int(gold["end_line"])
            and last >= int(gold["start_line"])
            for gold in golds
        )
    normalized_output_lines = {
        re.sub(r"^.*?(?::\d+)?:", "", line).strip()
        for line in output.splitlines()
        if line.strip()
    }
    for gold in golds:
        if address_found(output, gold):
            return True
        gold_path = gold["address"].split("#", 1)[0]
        for line in output.splitlines():
            if not line.startswith(f"{gold_path}:"):
                continue
            rest = line[len(gold_path) + 1 :]
            numbered = re.match(r"(\d+):", rest)
            if numbered:
                line_number = int(numbered.group(1))
                if int(gold["start_line"]) <= line_number <= int(gold["end_line"]):
                    return True
            elif rest.strip() in source_lines(gold):
                return True
        explicitly_searched = gold_path in {arg.replace("\\", "/") for arg in args}
        substantial_source_lines = {
            line for line in source_lines(gold) if len(line) >= 20
        }
        if explicitly_searched and normalized_output_lines & substantial_source_lines:
            return True
    return False


def quartiles(values: list[float]) -> dict[str, float]:
    ordered = sorted(values)
    if len(ordered) < 2:
        return {"median": ordered[0], "q1": ordered[0], "q3": ordered[0], "iqr": 0.0}
    q1, _q2, q3 = statistics.quantiles(ordered, n=4, method="inclusive")
    return {
        "median": statistics.median(ordered),
        "q1": q1,
        "q3": q3,
        "iqr": q3 - q1,
    }


@dataclass
class RunArtifacts:
    run_id: str
    arm: str
    answer: str
    transcript: list[dict[str, Any]]
    capped: bool
    run_dir: Path


def load_run(run_id: str, runs_root: Path) -> RunArtifacts:
    repetition, arm, task_id = run_id.split("-", 2)
    run_dir = runs_root / f"repetition-{repetition[1:]}" / arm / task_id
    answer_path = run_dir / "answer.txt"
    if not answer_path.exists():
        raise FileNotFoundError(f"missing answer: {answer_path}")
    transcript_path = run_dir / "transcript.jsonl"
    transcript = [
        json.loads(line)
        for line in transcript_path.read_text(encoding="utf-8").splitlines()
        if line
    ]
    return RunArtifacts(
        run_id=run_id,
        arm=arm,
        answer=answer_path.read_text(encoding="utf-8"),
        transcript=transcript,
        capped=(run_dir / "capped").exists(),
        run_dir=run_dir,
    )


def grade_artifacts(task: dict[str, Any], artifacts: RunArtifacts) -> dict[str, Any]:
    answer = artifacts.answer
    required_found = [
        gold["address"]
        for gold in task["required_addresses"]
        if address_found(answer, gold)
    ]
    supporting_found = [
        gold["address"]
        for gold in task["supporting_addresses"]
        if address_found(answer, gold)
    ]
    address_score = min(
        1.0,
        (len(required_found) + 0.5 * len(supporting_found))
        / len(task["required_addresses"]),
    )
    facts_found = [
        fact["id"] for fact in task["required_facts"] if fact_satisfied(answer, fact)
    ]
    fact_score = (
        len(facts_found) / len(task["required_facts"])
        if task["required_facts"]
        else None
    )
    penalty, forbidden_found = forbidden_penalty(answer, task["forbidden_claims"])
    raw = (
        address_score
        if task["type"] == "localization"
        else 0.5 * address_score + 0.5 * float(fact_score)
    )
    score = max(0.0, raw - penalty)
    navigation = None
    for record in artifacts.transcript:
        if call_retrieves(record, artifacts.arm, task["required_addresses"]):
            navigation = int(record["call_index"])
            break
    return {
        "task_id": task["id"],
        "arm": artifacts.arm,
        "answer": answer.rstrip(),
        "address_score": address_score,
        "fact_score": fact_score,
        "penalty": penalty,
        "score": score,
        "required_addresses_found": required_found,
        "supporting_addresses_found": supporting_found,
        "facts_found": facts_found,
        "forbidden_claims_found": forbidden_found,
        "tool_calls": len(artifacts.transcript),
        "tool_result_tokens": sum(
            int(r["result_tokens"]) for r in artifacts.transcript
        ),
        "navigation": navigation,
        "capped": artifacts.capped,
    }


def grade_run(task: dict[str, Any], arm: str, run_dir: Path) -> dict[str, Any]:
    """Harness-shaped entry point: same signature and output as grade.py's."""
    answer_path = run_dir / "answer.txt"
    if not answer_path.exists():
        raise FileNotFoundError(f"missing answer: {answer_path}")
    transcript_path = run_dir / "transcript.jsonl"
    transcript = [
        json.loads(line)
        for line in transcript_path.read_text(encoding="utf-8").splitlines()
        if line
    ]
    artifacts = RunArtifacts(
        run_id=f"-{arm}-{task['id']}",
        arm=arm,
        answer=answer_path.read_text(encoding="utf-8"),
        transcript=transcript,
        capped=(run_dir / "capped").exists(),
        run_dir=run_dir,
    )
    return grade_artifacts(task, artifacts)
