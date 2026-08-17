#!/usr/bin/env python3
"""Per-call metering wrapper: the only command a worker session may invoke.

subject_env() builds an allowlist environment rather than os.environ with
overrides: anything inherited is a variable whose value differs between
machines and operators, and a tool that reads one produces different output
-- or, as RIPGREP_CONFIG_PATH did, a warning that gets counted as result
tokens against a control arm. LC_ALL/LANG are pinned to C for deterministic
collation and byte handling, since rg and sed both sort and match
differently under a locale that is not C; TERM is pinned to "dumb" because
some tools probe it and emit escape sequences when it looks capable.

LINERANGE_TOOLS excludes `cat`: Arm C exists to price precise line-range
reads, and given `cat` it would collapse into Arm B and measure nothing the
baseline does not already measure.

require_subject_file resolves the path before the containment check, which
is what stops a symlink planted in the corpus from pointing out of it.

validate_linerange gates Arm C, where the worker gets `rg` to locate and
`sed` to read. `sed` is a programming language, not a pager: its `w` command
writes arbitrary files, `r` reads them, `e` runs a shell, and `-i` rewrites
the subject corpus in place, which would silently corrupt every run
scheduled after it. So validation does not filter dangerous forms out; it
admits exactly one form, `sed -n 'START[,END]p' FILE`, and refuses
everything else. SED_PRINT_RANGE matches `12p` or `12,80p` and nothing else,
anchored with fullmatch so no trailing `;w file` or `e cmd` can ride along
after the print command. An allowlist of one cannot be talked around.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

import tiktoken

from agentlens_evals.paths import (
    AGENTLENS,
    ARMS,
    DJANGO_ROOT,
    REPETITIONS,
    RUNS_ROOT,
    RunId,
)

CALL_CAP = 25
TOKEN_CAP = 60_000
AGENTLENS_TOOLS = {"slice", "map", "find", "literals", "callers", "packet", "dead"}
BASELINE_TOOLS = {"rg", "cat"}
LINERANGE_TOOLS = {"rg", "sed"}
ARM_TOOLS = {
    "agentlens": AGENTLENS_TOOLS,
    "baseline": BASELINE_TOOLS,
    "linerange": LINERANGE_TOOLS,
}
ENCODING = tiktoken.get_encoding("o200k_base")
SAFE_RG_SWITCHES = {
    "--case-sensitive",
    "--count",
    "--count-matches",
    "--files-with-matches",
    "--fixed-strings",
    "--ignore-case",
    "--invert-match",
    "--line-number",
    "--line-regexp",
    "--no-heading",
    "--only-matching",
    "--smart-case",
    "--stats",
    "--text",
    "--trim",
    "--vimgrep",
    "--word-regexp",
    "-F",
    "-H",
    "-I",
    "-L",
    "-N",
    "-S",
    "-c",
    "-i",
    "-l",
    "-n",
    "-o",
    "-s",
    "-v",
    "-w",
    "-x",
}
SAFE_RG_VALUE_OPTIONS = {
    "--glob",
    "--regexp",
    "--type",
    "--type-not",
    "-e",
    "-g",
    "-t",
    "-T",
}
SED_PRINT_RANGE = re.compile(r"\d+(,\d+)?p")


def subject_env() -> dict[str, str]:
    return {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "LC_ALL": "C",
        "LANG": "C",
        "NO_COLOR": "1",
        "TERM": "dumb",
    }


def die(message: str, code: int = 2) -> None:
    print(f"benchmark: {message}", file=sys.stderr)
    raise SystemExit(code)


def parse_run_id(raw: str) -> tuple[int, str, str]:
    parts = raw.split("-")
    if len(parts) != 3 or not parts[0].startswith("r"):
        die(f"run id must be r<1-{REPETITIONS}>-<{'|'.join(ARMS)}>-<task>")
    try:
        repetition = int(parts[0][1:])
    except ValueError:
        die("invalid repetition")
    arm = parts[1]
    task_id = parts[2]
    if repetition not in range(1, REPETITIONS + 1):
        die(f"repetition must be 1 through {REPETITIONS}")
    if arm not in ARMS:
        die(f"invalid arm: {arm!r} is not one of {', '.join(ARMS)}")
    if not (len(task_id) == 3 and task_id[0] in {"M", "L"} and task_id[1:].isdigit()):
        die("invalid task id")
    return repetition, arm, task_id


def safe_argument(argument: str) -> bool:
    path = Path(argument)
    return (
        not path.is_absolute()
        and ".." not in path.parts
        and ".agentlens-cache" not in path.parts
    )


def require_subject_file(argument: str, tool: str) -> None:
    candidate = (DJANGO_ROOT / argument).resolve()
    if not candidate.is_file() or not candidate.is_relative_to(DJANGO_ROOT):
        die(f"{tool} target is not a subject file: {argument}")


def validate_baseline(tool: str, args: list[str]) -> None:
    if any(not safe_argument(arg) for arg in args):
        die("absolute and parent-traversal arguments are forbidden")
    if tool == "cat":
        if not args or any(arg.startswith("-") for arg in args):
            die("cat accepts one or more repo-relative files and no options")
        for arg in args:
            require_subject_file(arg, "cat")
    if tool != "rg":
        return
    validate_rg(args)


def validate_linerange(tool: str, args: list[str]) -> None:
    if any(not safe_argument(arg) for arg in args):
        die("absolute and parent-traversal arguments are forbidden")
    if tool == "rg":
        validate_rg(args)
        return
    if tool != "sed":
        die(f"{tool!r} is unavailable in the linerange arm")
    if len(args) != 3 or args[0] != "-n" or not SED_PRINT_RANGE.fullmatch(args[1]):
        die("sed accepts exactly one form: sed -n 'START,ENDp' FILE")
    require_subject_file(args[2], "sed")


def validate_rg(args: list[str]) -> None:
    expecting_value = False
    for arg in args:
        if expecting_value:
            if not safe_argument(arg):
                die("unsafe rg option value")
            expecting_value = False
            continue
        if arg in SAFE_RG_VALUE_OPTIONS:
            expecting_value = True
            continue
        if arg.startswith("--") and "=" in arg:
            option, value = arg.split("=", 1)
            if option not in SAFE_RG_VALUE_OPTIONS or not safe_argument(value):
                die(f"rg option is unavailable: {option}")
            continue
        if arg.startswith("-") and arg != "-":
            if arg not in SAFE_RG_SWITCHES:
                die(f"rg option is unavailable: {arg}")
            continue
        if not safe_argument(arg):
            die("unsafe rg argument")
    if expecting_value:
        die("rg option is missing its value")


def append_jsonl(path: Path, record: dict[str, object]) -> None:
    with path.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(record, sort_keys=True, ensure_ascii=False))
        stream.write("\n")


def cap_result(text: str, remaining_tokens: int) -> tuple[str, int, int, bool]:
    encoded = ENCODING.encode(text)
    full_count = len(encoded)
    if full_count <= remaining_tokens:
        return text, full_count, full_count, False
    notice = ENCODING.encode("\n[benchmark tool-result token cap reached]\n")
    if remaining_tokens <= len(notice):
        limited = notice[:remaining_tokens]
    else:
        limited = encoded[: remaining_tokens - len(notice)] + notice
    rendered = ENCODING.decode(limited)
    return rendered, len(ENCODING.encode(rendered)), full_count, True


def submit(run_dir: Path, answer_parts: list[str]) -> None:
    if not answer_parts:
        die("submit requires the complete answer as one shell-quoted argument")
    answer = " ".join(answer_parts).strip()
    if not answer:
        die("answer cannot be empty")
    answer_path = run_dir / "answer.txt"
    if answer_path.exists():
        die("answer already submitted")
    answer_path.write_text(answer + "\n", encoding="utf-8")
    print("answer submitted")


def main() -> None:
    if len(sys.argv) < 4:
        die("usage: bench_tool.py RUN_ID TOOL ARGS...")
    run_id, tool, *args = sys.argv[1:]
    repetition, arm, task_id = parse_run_id(run_id)
    run_dir = RunId(repetition, arm, task_id).directory(RUNS_ROOT)
    run_dir.mkdir(parents=True, exist_ok=True)
    metadata_path = run_dir / "metadata.json"
    if not metadata_path.exists():
        metadata_path.write_text(
            json.dumps(
                {
                    "run_id": run_id,
                    "repetition": repetition,
                    "arm": arm,
                    "task_id": task_id,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
    if tool == "submit":
        submit(run_dir, args)
        return
    if tool not in ARM_TOOLS[arm]:
        die(f"{tool!r} is unavailable in the {arm} arm")
    transcript_path = run_dir / "transcript.jsonl"
    prior: list[dict[str, object]] = []
    if transcript_path.exists():
        prior = [
            json.loads(line)
            for line in transcript_path.read_text(encoding="utf-8").splitlines()
            if line
        ]
    cumulative = sum(int(record["result_tokens"]) for record in prior)
    if len(prior) >= CALL_CAP or cumulative >= TOKEN_CAP:
        (run_dir / "capped").touch()
        die("task cap reached; submit the best answer available", 3)
    if arm == "baseline":
        validate_baseline(tool, args)
        command = [tool, *args]
    elif arm == "linerange":
        validate_linerange(tool, args)
        command = [tool, *args]
    else:
        if AGENTLENS is None:
            die("BENCH_AGENTLENS is not set; refuse to guess the binary under test")
        if any(not safe_argument(arg) for arg in args):
            die("absolute and parent-traversal arguments are forbidden")
        command = [str(AGENTLENS), tool, *args]
    completed = subprocess.run(
        command,
        cwd=DJANGO_ROOT,
        check=False,
        capture_output=True,
        text=True,
        env=subject_env(),
    )
    full_result_text = completed.stdout + completed.stderr
    result_text, result_tokens, full_result_tokens, truncated = cap_result(
        full_result_text,
        TOKEN_CAP - cumulative,
    )
    record = {
        "call_index": len(prior) + 1,
        "tool": tool,
        "args": args,
        "exit_code": completed.returncode,
        "stdout": result_text,
        "stderr": "",
        "result_tokens": result_tokens,
        "full_result_tokens": full_result_tokens,
        "truncated_by_harness": truncated,
        "result_sha256": hashlib.sha256(result_text.encode()).hexdigest(),
        "full_result_sha256": hashlib.sha256(full_result_text.encode()).hexdigest(),
    }
    append_jsonl(transcript_path, record)
    if len(prior) + 1 >= CALL_CAP or cumulative + result_tokens >= TOKEN_CAP:
        (run_dir / "capped").touch()
    sys.stdout.write(result_text)
    raise SystemExit(completed.returncode)


if __name__ == "__main__":
    main()
