# agentlens benchmark — Django middleware (v2)

**Status: current specification. Pre-registered before the v0.2.0 run.**

This is the single source of truth for how the benchmark is run and scored.
Two earlier documents describe the v0.1.0 campaign and are superseded:
`evals/harness/RUBRIC.md` is kept as the frozen v1 pre-registration, and
`evals/django-golden.md` is now a redirect to this file.

Results are published separately in `EVAL_RESULTS_0_2_0.md`. This document says
what will be measured and what would count as failure; it does not report
numbers.

---

## 1. Objective

Three numbers per arm, over one task set, three arms:

| Metric | Definition | Direction |
|---|---|---|
| **Cost** | tool-result tokens consumed per rubric point earned | lower better |
| **Accuracy** | mean rubric score per task, 0.0–1.0 | higher better |
| **Navigation** | tool calls until the first gold address is retrieved | lower better |

The claim under test: *an agent answers the same questions about Django
middleware for materially fewer tokens using symbol addressing than using
conventional search-and-read, without losing accuracy.*

### 1.1 Pre-registered falsification conditions

There are two controls, and each gets its own threshold. agentlens **fails**
against a control if any of that control's conditions holds.

| Condition | vs Arm B (`rg` + `cat`) | vs Arm C (`rg` + `sed`) |
|---|---|---|
| Cost ratio at or above | 0.5 | 0.7 |
| Accuracy below the control by more than | 0.05 | 0.05 |
| Outright losses on more than | 4 of 18 tasks | 6 of 18 tasks |

These live in machine-readable form in `evals/harness/protocol.json` under
`falsification`, and the grader reads them from there rather than from prose,
so the published verdict cannot drift from the pre-registration.

**Why Arm C's bar is lower.** The 0.5 threshold was set against Arm B, which
the v1 spec itself called the weaker of the two realistic baselines. Arm C is
the disciplined baseline a competent agent actually uses: it pays for precise
reads, but it still pays for the searches that locate the lines first. Demanding
a 2x win against it would be setting a bar the claim never made. A modest rather
than dramatic advantage is the honest prior, so 0.7 is what gets pre-registered.

Asymmetric thresholds are a hazard — they let an author pick the number that
passes. The mitigation is that both are fixed here, in advance, and reported
side by side with equal prominence regardless of which one is kinder.

---

## 2. Subject under test

| | |
|---|---|
| Repository | `django/django` |
| Commit | `e2a424605ac2e7e6e799496542fb2997207e2f23` (tag 6.0.7) |
| Checkout | outside the agentlens tree, at `$BENCH_DJANGO_ROOT` |
| Feature area | request/response middleware |
| Language | Python — agentlens's only supported language (see §8) |

The commit is frozen. Gold line spans are recorded against it, so re-pinning
would silently invalidate every address match.

The checkout lives outside the repository on purpose. Inside the tree it would
be walked by `rg`, indexed by `agentlens dead`, and would contaminate all three
arms at once.

Middleware is the chosen area because it is *diffuse*: chain assembly in
`django/core/handlers/base.py`, exception conversion in
`django/core/handlers/exception.py`, the sync/async shim in
`django/utils/deprecation.py`, roughly ten concrete middleware in
`django/middleware/`, plus settings defaults, system checks and tests. A
single-file feature would be a trivial win for any tool.

---

## 3. Arms

All three arms are the same model, the same system prompt and the same task
text. The **only** difference is the toolset, and one sentence in the prompt
describing it.

| Arm | Tools | Rationale |
|---|---|---|
| **A — agentlens** | `slice`, `map`, `find`, `literals`, `callers`, `packet`, `dead` | The tool under test. No file reads, no grep. |
| **B — baseline** | `rg <pattern> [path]`, `cat <file>` | The status quo fallback. No line-range reads. |
| **C — linerange** | `rg <pattern> [path]`, `sed -n 'START,ENDp' FILE` | The disciplined baseline. No whole-file reads. |

Arm C exists because the v1 spec pre-registered its own trigger for adding it:
*"If the cost ratio comes out below 0.25, that arm should be added before the
number is quoted anywhere."* v0.1.0 came out at 0.1706, so Arm C is owed.

**Arm C deliberately excludes `cat`.** With `cat` available, Arm C is Arm B
plus an option, and the arms stop being distinct. Removing whole-file reads is
what makes it a separate strategy rather than a superset.

Arm A's index cache is warmed once before the run. Warm cache is the realistic
steady-state condition; the cold build cost is measured separately and reported
as a footnote so anyone can adjust the headline number for it.

### 3.1 The sed validator

`sed` is a programming language. An unvalidated `sed` is arbitrary file read
*and* arbitrary write and execution, via the `w`, `r` and `e` commands. The
wrapper therefore accepts exactly one form and rejects everything else:

```
sed -n 'START,ENDp' FILE
```

Three arguments, the first exactly `-n`, the second matching `\d+(,\d+)?p` in
full, the third resolving to a file inside the subject checkout. `-e`, `-f`,
`-i`, embedded `w`, absolute paths and parent traversal are all rejected.
Adversarial tests for each of these were written before the validator.

---

## 4. Task set

18 tasks: 12 comprehension, 6 localization. Every task is answerable from the
source alone, and every task is derived from a question Django's own
documentation answers rather than invented to suit the tool.

The task set, gold addresses, gold line spans, required facts and forbidden
claims live in `evals/harness/tasks.json`. That file is a **frozen input**. It
was verified against the pinned commit — every gold address resolves — and
hashed into `gold.sha256` and `gold.blake3` before any arm ran. It is unchanged
from v0.1.0, which is what makes the two campaigns comparable.

Task records carry: `id`, `type`, `prompt`, `required_addresses`,
`supporting_addresses`, `required_facts` (each with an `any_of` phrase list),
`forbidden_claims`, and `source_of_truth`.

---

## 5. Scoring

### 5.1 Per-task score

```
address_score = ( |found ∩ required| + 0.5·|found ∩ supporting| ) / |required|
                clamped to [0, 1]

fact_score    = |facts asserted correctly| / |required_facts|

raw           = 0.5 · address_score + 0.5 · fact_score

penalty       = 0.25 × (forbidden claims asserted confidently)

score_t       = max(0, raw − penalty)
```

Localization tasks have no `required_facts`; for those,
`score_t = address_score − penalty`.

The penalty exists because a confidently wrong answer is worse than no answer:
it costs the caller a wasted turn and corrupts their model of the code. An arm
that hedges everything scores low on facts; an arm that bluffs scores low on
penalty.

### 5.2 Address matching

**Address matching must not require agentlens's address syntax.** Arms B and C
cannot emit `file.py#Class.method`, and grading them as though they should
would rig the benchmark. An address counts as found if the answer contains any
of:

- the symbolic form `path#Dotted`, path normalised and repo-relative, or
- a `path` plus a line or line range overlapping the gold symbol's span, or
- a `path` plus the bare symbol name within 100 characters of it.

A line citation is recognised in any of the forms an answer actually uses:
`L40-L50` and `:40-50`, which is how agentlens prints an address, and the prose
forms `lines 40-50`, `at line 40`, `lines 40 to 50`. The prose forms were added
after the v0.2.0 smoke run exposed their absence: the linerange arm answered
M07 by citing `base.py` lines 205-222 against a gold span of 175-227 — correct,
and fully inside the span — and scored zero, purely because it did not spell the
citation the way the tool under test does. **This bias ran in agentlens's favour
and was present in v0.1.0**, so v0.1.0's published address scores understate both
control arms. The prose form requires the word "line" or "lines"; a bare number
beside a path ("Django 6.0.7") is not a citation.

Span boundaries come from the frozen gold table generated at verification time.

### 5.3 Fact matching — matcher v2

This changed between campaigns, and the change is large enough that v1 and v2
fact scores are not interchangeable. Address matching changed too, for a
different reason — see §5.2.

**Why it changed.** v1 matched `any_of` phrases as case-folded substrings. That
recognised **12.7%** of required facts across all arms — an answer saying
"shorter than 200 bytes" missed the frozen phrase "less than 200 bytes". A
grader that misses seven facts in eight is not measuring accuracy, it is
measuring whether a model happened to reuse the rubric author's wording.

**What v2 does.** All mechanical, no LLM judge:

- Case folding, whitespace collapse and punctuation stripping.
- Light suffix stemming, not a full lemmatizer.
- Comparative synonym classes — `{shorter, smaller, less, under, below, fewer}`,
  `{longer, larger, greater, more, over, above}` and 15 others.
- Numeric and unit equivalence: `200 bytes` ≡ `200-byte` ≡ `200B`.
- Order-insensitive token-subset matching inside a bounded window, so "response
  shorter than 200 bytes" matches "less than 200 bytes in the response".
- Negation parity: a phrase and its negation do not match each other.

**Known limitation, stated rather than hidden.** v2 matches on shared content
words. A genuine paraphrase sharing no content noun — "gzip result" against
"compressed content" — is not matched. There is a test asserting exactly this,
so the boundary is documented rather than discovered later.

The hedge and penalty logic is **unchanged** from v1. Only recall of positive
assertions was broken, and only that was repaired.

### 5.4 Aggregation

```
Accuracy      = mean(score_t) over 18 tasks
Cost          = Σ tool_result_tokens / Σ score_t          ← tokens per point
Navigation    = median over tasks of (calls until first gold address retrieved)
Head-to-head  = per-task count of score_A greater than / less than / equal to control
```

Cost divides by *points earned*, not tasks attempted. An arm that saves tokens
by answering nothing gets no credit.

Campaigns are compared as **per-repetition medians, never pooled**. v0.1.0 has
3 repetitions and v0.2.0 has 5; pooling would weight them unequally.

---

## 6. Measurement rules

**Token accounting.** Count tokens in **tool results injected into context** —
the bytes the tool forces the caller to pay for. Counted with an external
tokenizer (`tiktoken`, `o200k_base`), never with agentlens's own
`estimate_tokens`. Letting the tool score its own cost metric would be
indefensible.

Not counted: model reasoning tokens, task prompt, system prompt. These are
near-identical across arms and would dilute the effect.

**Environment isolation.** Every measured subprocess runs under an explicit
environment allowlist — `PATH`, `LC_ALL`, `LANG`, `NO_COLOR`, `TERM` — and
nothing else. This is not hygiene theatre. In v0.1.0 the operator's
`RIPGREP_CONFIG_PATH` leaked into measured commands, and the resulting warning
was billed as result tokens against a control arm. See §8.

**Caps.** 25 tool calls or 60,000 tool-result tokens per task, whichever comes
first. On cap the task is scored on whatever the arm produced and flagged
`capped: true`. Capped tasks are reported separately *and* included in totals.
A cap is a result, not a discard. Caps are unchanged from v1 and are part of
the frozen comparison.

**Repetitions.** 5 independent runs per task per arm — 18 × 3 × 5 = **270
sessions** — each a fresh session with no memory across tasks. Report median
and interquartile range. Single-run agent benchmarks are noise.

**Randomisation.** Task order is shuffled per repetition from a recorded seed
(`20260730`), and arm order is rotated by repetition rather than alternated,
since there are three arms. The full schedule is generated deterministically by
`make_schedule.py` and frozen in `schedule.json`; regenerating it reproduces
the file byte for byte.

**Determinism check.** agentlens output is deterministic by design. The harness
asserts byte-identical tool output across repetitions and flags drift as a tool
bug rather than averaging over it.

---

## 7. Integrity controls

The uncomfortable fact underneath all of these: I built the tool and I wrote
the grader. Every control below exists because that is a conflict of interest,
not because it is good practice in the abstract.

**1. Gold frozen and hashed before any arm runs.** `gold.sha256` and
`gold.blake3` are recorded in the results. Changing gold after the run shows up
as a hash mismatch.

**2. Grading is mechanical.** A script matches addresses and facts. No model
judges any answer, in either direction. Judge variance would swamp the effect
being measured, and a judge is exactly where an author's thumb would rest.

**3. The matcher was tuned blind.** This is the highest-risk change in the
campaign: editing the grader that scores my own tool. The procedure was:

- Extract every v0.1.0 answer with its `(task_id, fact_id)` pairs and **strip
  the arm label** before the tuner sees anything. The sample type has no arm
  field at all, so it cannot be consulted by accident.
- Split tasks into a dev half and a held-out half, stratified by task type,
  from a fixed seed.
- Tune only against dev. Touch held-out once, at the end.
- Report per-arm recall change v1→v2 in the results. **If v2 lifts agentlens
  materially more than it lifts the controls, that asymmetry is a finding to
  publish, not to suppress.**

Held-out recall (0.806) came out *above* dev recall (0.739), which is evidence
against overfitting to the tuning half.

**4. Worker isolation is enforced, not requested.** Three independent layers,
described in §7.1.

**5. No agentlens self-measurement.** External tokenizer, external grader.

**6. Raw transcripts retained** for all 270 runs, so any published number can
be audited back to the tool calls that produced it.

**7. Author bias, disclosed.** Tasks derive from Django's own documentation
sections rather than being invented; each gold answer cites a `source_of_truth`
path; falsification conditions are pre-registered above; and the threats in §8
are stated rather than buried.

### 7.1 Worker isolation

A worker that reads the subject outside the metering wrapper consumes tokens
nobody counts. Token cost is the metric the benchmark exists to report, so a
single unmetered `cat` of a large module silently makes an arm look cheaper
than it was. Prompt instructions are not enforcement.

**Layer 1 — the prompt.** Each worker is told to use only the wrapper.

**Layer 2 — a PreToolUse gate.** A hook denies any tool call naming the subject
checkout or the harness's own answer key, unless it goes through
`bench_tool.py`. It covers `Bash`, `Read`, `Grep` and `Glob`, because a worker
dispatched with a general toolset reaches the subject through all four. The
gate is deliberately narrow — it allows everything that does not name the
subject or gold — so that it is never in the way enough to be switched off.

**Layer 3 — post-hoc leak detection.** The authoritative check, because it
reads what happened rather than what was supposed to. Every repo-relative
source path an answer cites must appear somewhere in that run's own captured
tool output. An answer naming a file the run never retrieved either came from
the model's prior knowledge of Django — which is memorisation, not navigation —
or from an unmetered read.

Flagged runs split into two classes, because they need different remedies:

- **Breach** — the run still had budget and cited a file it never retrieved.
  Quarantined and re-run, and the count published.
- **Memorisation under cap** — the run hit the 25-call or 60k-token cap, and
  its own prompt then told it to submit the best answer available. An
  unretrieved citation here is the cap talking, not a bypass; with the gate in
  place there was no unmetered read on offer. **Reported and retained, not
  re-run.** Re-running a capped run is indistinguishable from re-rolling a hard
  task until the tool under test looks better — which matters most precisely
  when the flagged run is in Arm A, as it is in this campaign.

Silently dropping either class would bias the sample toward whatever the tool
happens to be good at, so both counts are published per arm.

The leak detector is conservative by design: it flags cited paths only, never
prose, because a worker may legitimately describe behaviour in words it never
saw verbatim.

### 7.2 Worker configuration — and what is not proven

Workers are dispatched as Claude Code subagents pinned to
`claude-haiku-4-5-20251001`, one fresh session per run, with the run ID baked
into every wrapper invocation.

**The model identity is pinned by dispatch configuration and recorded. It is
not independently proven.** A subagent cannot produce a runtime model-ID
receipt. This was also true of v0.1.0 and is not fixed here; it is stated so
nobody mistakes the attestation for verification.

The v0.1.0 campaign used a different worker model. That, plus the matcher
change and the environment leak, is why cross-campaign comparison is reported
with all three caveats attached rather than as a clean before/after.

---

## 8. Threats to validity

| Threat | Effect on result |
|---|---|
| **Django is Python** | agentlens's only supported language, and its best case. The result does not generalise to Go, Rust, TypeScript or Java. |
| **Middleware is symbol-shaped** | Classes and methods with clean boundaries, ideal for `slice`. A config-driven or metaprogramming-heavy feature would favour the tool much less. |
| **Author-written gold** | See §7.7. Mitigated by documentation-derived tasks and per-task `source_of_truth`, not eliminated. |
| **Baseline prompt quality** | How well Arms B and C are instructed materially changes their cost. Both prompts are written to be genuinely competent and are published verbatim. |
| **Whole-file size drives Arm B's cost** | `django/core/handlers/base.py` is large; a repo of small files would narrow the gap. File sizes touched are reported alongside totals. |
| **Warm cache** | Arm A runs warm. Cold-build cost is reported as a footnote so the headline can be adjusted. |
| **Matcher changed between campaigns** | Both fact matching (§5.3) and address matching (§5.2) changed. v1 and v2 scores are not interchangeable. v0.1.0 is re-graded under v2 and reported under both, and the per-arm recall change is published (§7.3). Both changes were made before any v0.2.0 number existed, and the address change corrects a bias that favoured agentlens. |
| **Worker model pinned but unproven** | §7.2. Dispatch configuration is recorded; there is no runtime receipt. |
| **v0.1.0 ran under a leaked environment** | `RIPGREP_CONFIG_PATH` from the operator's shell reached measured commands and its warning output was billed to a control arm. v0.1.0's published cost figures are therefore slightly *favourable to agentlens* and should be treated as provisional. v0.2.0 uses an environment allowlist. |
| **v0.1.0 used a different worker model** | Cross-campaign deltas confound tool version with model. |

The v1 threat **"no line-range baseline"** is retired: Arm C now exists, and
the measured advantage is no longer an upper bound taken against the weaker
control alone.

---

## 9. Run protocol

0. **Verification gate.** Resolve every gold address against the pinned commit.
   Freeze and hash gold. **Nothing runs until this passes.**
1. Build the release binary from the merged stack; record its commit and
   SHA-256 in `protocol.json`.
2. Warm the agentlens index once; record the cold-build cost.
3. **Smoke test.** Execute one task across all three arms and inspect the
   transcripts by hand: arm isolation held, token counts non-zero and
   plausible, `submit` wrote `answer.txt`. Do not launch the campaign until
   this passes.
4. Execute 270 runs in schedule order, batched by repetition. Runs are
   resumable: an existing `answer.txt` means complete and is skipped, never
   redone.
5. Run leak detection over all 270. Quarantine and re-run anything flagged.
6. Grade mechanically. Re-grade the frozen v0.1.0 transcripts with the same v2
   matcher.
7. Publish `EVAL_RESULTS_0_2_0.md`: verdict against every pre-registered
   condition first, then the numbers.

---

## 10. Reproducing this

```bash
uv sync --directory evals/harness

uv run --directory evals/harness python verify_gold.py       # hard gate
uv run --directory evals/harness python validate_protocol.py # 270 attestations
uv run --directory evals/harness python -m unittest discover # harness + gate tests
uv run --directory evals/harness python plan_runs.py --status
uv run --directory evals/harness python detect_leaks.py
uv run --directory evals/harness python grade.py
```

Paths are resolved in one place, `evals/harness/paths.py`, and overridable with
`BENCH_DJANGO_ROOT`, `BENCH_AGENTLENS` and `BENCH_RUNS_ROOT`. Point
`BENCH_RUNS_ROOT` at the v0.1.0 corpus to audit or re-grade the frozen campaign.
