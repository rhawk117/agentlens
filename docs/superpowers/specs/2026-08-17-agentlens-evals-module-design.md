# agentlens-evals: a formal, re-runnable benchmark module

Status: draft for review. Branch: `eval/pydantic-evals-module` off `dev`.

## Why this exists

The v0.2.0 benchmark campaign had to be thrown away and re-run. The wrapper
resolved the binary under test from `target/release/agentlens`, a path that any
`cargo build` mutates, and the binary was in fact rebuilt mid-campaign
(mtime 2026-07-30 23:56, inside the 23:42–00:34 run window). No attestation can
say which runs used which binary. Separately, campaigns are orchestrated by an
interactive Claude Code session improvising over `attest.py`, `plan_runs.py`,
and subagent dispatch, so no two campaigns are guaranteed to run the same way.

This module makes a campaign a command: reproducible between versions,
impossible to point at an unpinned binary, and independent of any interactive
session.

## Decisions already made (with the user, 2026-08-17)

These were settled through explicit questions; do not reopen them.

1. **Port, not wrap, not rewrite.** The grading and campaign logic moves into a
   new pydantic-evals-based package. `bench_tool.py` (the metering gate) is
   kept unchanged in behavior. A greenfield redesign was rejected because it
   breaks comparability with the v0.1.0/v0.2.0 campaigns; a thin wrapper was
   rejected because it leaves two systems to maintain.
2. **Workers run via the Python `claude-agent-sdk`**, spawned by the module
   itself. Keeping Claude Code session dispatch was rejected because it makes
   re-runs depend on an interactive session.
3. **Worker model is `claude-haiku-4-5-20251001`, pinned as a code constant.**
   Never the session model, never Fable 5. This is both a cost decision (the
   user cut a prior campaign from 5 to 3 repetitions over expense) and a
   comparability decision (v1/v2 attestations record Haiku 4.5).
4. **Binary under test lives in gitignored `evals/bin/<version>/`** with a
   manifest, not in `.eval/`. `evals/` holds everything eval-shaped; `.eval/`
   stays the runs corpus.

## Architecture

New uv package: `evals/agentlens_evals/` (Python 3.12+, managed with uv,
`pydantic-evals` and `claude-agent-sdk` as dependencies), console script
`agentlens-evals`. The existing `evals/harness/` is frozen as the record of the
v1/v2 campaigns and is not deleted; the new module is the only way future
campaigns run.

### Components

- **`subject.py` — binary under test.** `evals/bin/<version>/` contains the
  `agentlens` binary and `manifest.json` recording version, sha256, source
  commit, and build time. `agentlens-evals install --version X` builds via
  cargo and writes both. Every campaign command re-hashes the binary and
  refuses to run on any mismatch with the manifest and the campaign protocol.
  `evals/bin/` is gitignored; `target/release/` is never consulted.
- **`dataset.py` — tasks as pydantic models.** `tasks.json` (18 tasks, two
  types: comprehension and localization; gold addresses with line spans,
  required facts with accepted phrasings, forbidden claims) loads into typed
  models and a pydantic-evals `Dataset` with one `Case` per task. Arm
  (`agentlens`, `baseline`, `linerange`) and repetition are experiment
  dimensions: one evaluate pass per arm × repetition.
- **`metering.py` — the gate, ported verbatim.** The current `bench_tool.py`
  semantics are preserved exactly: per-run transcript JSONL, 25-call cap,
  60k tool-result-token cap (tiktoken `o200k_base`), arm-specific tool
  allowlists (`rg`/`cat`, `rg`/`sed -n 'START,ENDp'` only, agentlens
  subcommands), argument validation, submit-once. It remains the only door to
  the subject corpus.
- **`worker.py` — one benchmark session.** Spawns a Haiku 4.5 session through
  `claude-agent-sdk`: Bash as the only tool, a permission callback that allows
  nothing but wrapper invocations (the SDK equivalent of the
  `eval_subject_gate.py` hook), and the rendered worker prompt (ported from
  `attest.py:render_prompt`). The worker researches through the wrapper and
  submits; the answer and transcript land on disk.
- **`evaluators.py` — graders as pydantic-evals evaluators.** Direct ports:
  address matching (exact address, or path plus symbol/overlapping line
  reference in a ±100-char window), fact matching (matcher v2 from
  `matching.py`: canonical token sets in a window, stopword strip, light
  suffix stemming, synonym classes, negation parity), forbidden-claim
  penalties (0.25, or 0.1 when hedged), navigation index (first metered call
  whose output retrieves a gold span, per-arm retrieval rules), and cost
  (tool calls, result tokens). Score formula unchanged: localization =
  address score; comprehension = mean of address and fact scores, minus
  penalties, floored at 0.
- **`campaign.py` — orchestration.** Owns the schedule (ported from
  `make_schedule.py`), a per-campaign runs root (`.eval/runs_<label>/`),
  resumability, attestation events, and post-hoc leak detection (ported from
  `detect_leaks.py`). Concurrency is bounded and configurable.
- **`report.py` — results and falsification.** Per-arm quartiles, per-task
  head-to-head, cost ratio and accuracy delta against each control
  separately (controls are never pooled), judged against the pre-registered
  falsification thresholds in `protocol.json`. Emits `results.json` and a
  markdown summary.

### CLI surface

    agentlens-evals install --version 0.2.0    # build + manifest into evals/bin/
    agentlens-evals run --tool-version 0.2.0 --repetitions 3 [--resume]
    agentlens-evals status                      # complete / stranded / pending
    agentlens-evals grade [--runs-root ...]
    agentlens-evals report

### Resumability: disk is the source of truth

pydantic-evals wants to own per-case concurrency and retries. We let it drive
execution, but the on-disk layout (`answer.txt`, `transcript.jsonl`, `capped`)
remains authoritative, exactly as today: a run with an answer is complete and
is never re-dispatched; a run with a transcript but no answer resumes against
its remaining call/token budget. pydantic-evals reports are derived artifacts.
This keeps a crashed or interrupted campaign resumable from disk regardless of
what the framework's in-memory state was.

## Verification gates

1. **Matcher-port parity (hard gate):** grading the completed `runs_v2` corpus
   with the new evaluators must reproduce `grade.py`'s per-run scores exactly
   (matcher version 2). Any divergence is a port bug until proven otherwise.
2. **Gate tests:** port `test_gate.py`/`test_harness.py` coverage — arm tool
   allowlists, sed single-form allowlist, path traversal refusals, caps,
   submit-once.
3. **Binary refusal test:** a campaign command pointed at a binary whose hash
   does not match its manifest must refuse to start.
4. **Worker-model assertion:** the dispatch path must fail loudly if the
   configured model is not the pinned Haiku 4.5 identifier.

## Constraints and facts the implementer needs

- The subject corpus is `~/dev/django-6.0.7` (override `BENCH_DJANGO_ROOT`),
  pinned to tag `6.0.7`, commit `e2a424605ac2e7e6e799496542fb2997207e2f23`.
  It deliberately lives outside the repo so no arm indexes or walks it.
- Grading requires the corpus present (retrieval checks read gold spans from
  source files).
- `.claude/hooks/eval_subject_gate.py` gates this repo's interactive sessions.
  It denies tool calls naming the corpus or answer-key files
  (`tasks.json`, `gold.*`, `results*.json`, `RUBRIC.md`, `protocol.json`,
  `schedule.json`, `attestations*.jsonl`, `METHODOLOGY.md`,
  `django-golden.md`); `BENCH_GATE=off` is the orchestrator override. Module
  code is unaffected (hooks only bind interactive sessions), but any agent
  implementing this will trip the gate if its shell commands name those files.
- The tokenizer is pinned: tiktoken `o200k_base`, tiktoken 0.8.0. Token
  counts are the headline metric; changing the tokenizer breaks comparability.
- Pre-registration discipline: `schedule.json` stays at 5 repetitions even
  though campaigns execute 3; thresholds in `protocol.json` are set before a
  campaign and never edited to match an outcome. The new module must not
  regenerate or rewrite either during a run.
- The eval-worker prompt semantics (research only through the wrapper, submit
  exactly once, answer format per task type, no hedging) are in
  `attest.py:render_prompt` and `.claude/agents/eval-worker.md`.

## Not yet investigated

- Exact current pydantic-evals API (Dataset/Case/Evaluator signatures,
  concurrency controls, report rendering). Verify against the installed
  version during planning; the mapping above is conceptual.
- Exact `claude-agent-sdk` Python API for tool restriction and permission
  callbacks. Verify during planning.
- Whether `attest.py`'s sequential start-order enforcement should carry over
  as-is once dispatch is programmatic (the ordering existed to pin an
  interactive orchestrator; a deterministic driver may make it redundant).
  Default: keep it, drop only with explicit sign-off.

## Out of scope

- Re-grading or restructuring the frozen v1/v2 campaigns beyond the parity
  check in the verification gates.
- New tasks, arms, or rubric changes.
- CI integration (a campaign spends real API money; runs stay operator-initiated).
