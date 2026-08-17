---
name: running-eval-campaigns
description: Use when running, resuming, grading, or reporting an agentlens benchmark campaign — a version bump needing a re-run ("run the evals for vX.Y.Z"), an interrupted or stranded campaign, or a question about whether a version passes the benchmark.
---

# Running Eval Campaigns

## Overview

Campaigns run only through the `agentlens-evals` CLI (package `evals/agentlens_evals/`). The scripts in `evals/harness/` are the frozen v1/v2 record — never drive a new campaign with them.

Campaign shape: **162 runs** = 18 tasks × 3 arms (agentlens / baseline / linerange) × **3 executed repetitions**. `schedule.json` stays pre-registered at 5 repetitions; the module truncates to 3. `status` counts against 162 — a "270" expectation means you are misreading the schedule.

All commands from `evals/agentlens_evals/`, or prefix `uv run --project evals/agentlens_evals`.

## Command sequence

| Step | Command | Why |
|---|---|---|
| Preconditions | `Cargo.toml` version == X; corpus pin (below); `uv run pytest -q` green (44 tests) | catch drift before spending budget |
| Pin binary | `agentlens-evals install --version X` | cargo build → `evals/bin/X/` + sha256 manifest; every later command re-hashes and refuses on mismatch |
| Fresh runs root | pass `--runs-root .eval/runs_<label>` on **every** command | the default is `.eval/runs_v2` — the completed v0.2.0 corpus. Reusing it silently reports old answers as the new campaign |
| Dispatch | `agentlens-evals run --tool-version X --runs-root … --concurrency 4` | the **only** command that spends API money. Workers are pinned `claude-haiku-4-5-20251001`; dispatch refuses any other model |
| Verdict | `agentlens-evals grade --runs-root …` | writes `<runs-root>/results.json`; `benchmark_failed: false` = passes all pre-registered thresholds |
| Report | `agentlens-evals report --runs-root …` | prints the markdown verdict **followed by** derived pydantic-evals tables — do not redirect wholesale into a results doc; take the markdown section only |

**Corpus pin:** the subject checkout must be commit `e2a424605ac2e7e6e799496542fb2997207e2f23` with a clean tree. **Nothing verifies this automatically** — install/run/grade check the binary hash and gold hashes, never the corpus commit; this manual check is the only guard. The command names the corpus, so the gate denies it; this check is the sanctioned use of the override:

```bash
BENCH_GATE=off git -C ~/dev/django-6.0.7 rev-parse HEAD && BENCH_GATE=off git -C ~/dev/django-6.0.7 status --short
```

(`BENCH_GATE=off` must be the very first token; Bash only.)

## Interruption and refusals

- **Interrupted / stranded runs:** re-run the same `run` command verbatim. Complete = `answer.txt` exists (never re-dispatched); stranded = transcript without answer (resumes against its remaining call/token budget). No separate recovery command exists.
- **`SubjectError` (hash mismatch):** someone rebuilt the binary. Completed runs stay valid — the check fires before dispatch. `install` again, then re-run `run`.
- **`run` exits 1 = breaches:** an uncapped run cited a path its own transcript never retrieved. Quarantine and re-dispatch those runs; do not grade around them. Capped "memorisation" findings are *reported*, never re-rolled.

## Never

- `cargo build` while a campaign is in flight (the v0.2.0 contamination).
- Edit or regenerate `schedule.json` / `protocol.json` — pre-registration is only meaningful if outcomes can't rewrite it. Cutting repetitions for cost is recorded in prose, not by editing the schedule.
- Point anything at `target/release/agentlens`.

## Repo gate hook

Interactive tool calls (Bash/Read/Grep/Glob) naming the gold files (task JSON, gold digests, protocol, schedule, results, attestations, methodology) or the Django corpus are denied by `.claude/hooks/eval_subject_gate.py`. Don't fight or bypass it — the module reads those files itself, and `uv run pytest`/`agentlens-evals …` command lines don't trip it. `BENCH_GATE=off` (first token, Bash only) is for deliberate orchestrator use: the corpus-pin check above is the sanctioned example. Never use it to read gold files.
