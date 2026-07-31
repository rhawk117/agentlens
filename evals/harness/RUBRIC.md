# agentlens benchmark — Django middleware

> **Superseded. This is the frozen v1 record — do not edit it.**
>
> This document is the pre-registration for the **v0.1.0** campaign, written
> before that campaign ran. It is kept unedited so the v0.1.0 results remain
> auditable against the specification they were produced under, including the
> parts that were still open questions at the time.
>
> It is **not** an accurate description of how the benchmark runs today. It
> describes two arms, three repetitions, an unpinned commit and unverified
> gold. The current specification — three arms, five repetitions, the pinned
> commit, verified gold, matcher v2 and enforced worker isolation — is
> [`evals/METHODOLOGY.md`](../METHODOLOGY.md).
>
> Two notes for anyone reading the v0.1.0 numbers against this text. Gold *was*
> verified against the pinned commit before that campaign ran, despite the
> DRAFT markings below; and the open questions in §10 were all answered, in
> §1.1 and §3 of the current specification.

**Status: superseded by evals/METHODOLOGY.md. Frozen v1 pre-registration.**

Gold answers below are drafted from knowledge of Django's middleware architecture
and are marked unverified. Verification against the pinned commit is step 0 of the
run protocol and is a hard gate: no arm executes until every gold address resolves.

---

## 1. Objective

Three numbers from one run, over one task set, two arms:

| Metric | Definition | Direction |
|---|---|---|
| **Cost** | tool-result tokens consumed per rubric point earned | lower better |
| **Accuracy** | mean rubric score per task, 0.0–1.0 | higher better |
| **Navigation** | tool calls until the first gold address is retrieved | lower better |

The headline claim under test: *an agent answers the same questions about Django
middleware for materially fewer tokens using symbol addressing than using grep and
whole-file reads, without losing accuracy.*

### Pre-registered falsification condition

agentlens **fails** this benchmark if any of the following holds:

- Cost ratio ≥ 0.5 (i.e. it does not at least halve tokens per point), **or**
- Accuracy is more than 0.05 below the baseline arm, **or**
- It loses outright (lower score) on more than 4 of the 18 tasks.

Stating this before the run is the point. A benchmark whose author cannot say in
advance what losing looks like is a demo, not an experiment.

---

## 2. Subject under test

| | |
|---|---|
| Repository | `django/django` |
| Commit | **pinned at run time, recorded in results — TBD** |
| Feature area | request/response middleware |
| Language | Python (agentlens's only supported language — see §8) |

Middleware is chosen because it is *diffuse*. The behaviour spans chain assembly
(`django/core/handlers/base.py`), exception conversion
(`django/core/handlers/exception.py`), the sync/async deprecation shim
(`django/utils/deprecation.py`), roughly ten concrete middleware in
`django/middleware/`, settings defaults, system checks, and tests. A single-file
feature would be a trivial win for any tool; this one requires multi-hop traversal.

---

## 3. Arms

Both arms are the same agent, same model, same system prompt, same task text. The
**only** difference is the toolset.

### Arm A — `agentlens`

Tools: `slice`, `map`, `find`, `literals`, `callers`, `packet`, `dead`.
No file reading, no grep. Index cache pre-warmed once before the run (warm-cache is
the realistic condition; cold-start cost reported separately as a footnote).

### Arm B — grep + whole-file reads (baseline)

Tools: `rg <pattern> [path]` and `cat <file>`.
No line-range reads. This is the status quo an agent falls back to today.

### Deliberately excluded

A `rg` + `sed -n 'X,Yp'` arm — a disciplined line-range baseline — would be the
harder, fairer comparison. It is **not** in this run. Its absence is a known
weakness of the result and is recorded in §8 rather than quietly omitted. If the
cost ratio comes out below 0.25, that arm should be added before the number is
quoted anywhere.

---

## 4. Task set

18 tasks: 12 comprehension, 6 localization. Every task is answerable from the
source alone. Tasks are derived from questions Django's own documentation answers,
not invented to suit the tool.

### 4.1 Task record schema

```yaml
id: M07
type: comprehension            # comprehension | localization
prompt: >
  A middleware's __call__ returns None instead of a response.
  What happens, and which code raises or surfaces the error?
required_addresses:            # each worth an equal share of the address half
  - django/core/handlers/base.py#BaseHandler._get_response
  - django/core/handlers/base.py#BaseHandler.check_response
supporting_addresses:          # half credit each, capped at the address half
  - django/core/handlers/exception.py#convert_exception_to_response
required_facts:                # substring-or-synonym matched, see 5.2
  - id: F1
    any_of ["ValueError", "returned None"]
  - id: F2
    any_of ["check_response", "response validation"]
forbidden_claims:              # confident assertion of these costs a penalty
  - "the chain silently continues"
  - "returns an empty 200"
source_of_truth: docs/topics/http/middleware.txt
```

### 4.2 Comprehension tasks (12) — DRAFT, UNVERIFIED

| ID | Question | Primary gold addresses (draft) |
|---|---|---|
| M01 | In what order are middleware applied on the request path vs the response path, and where is that order established? | `core/handlers/base.py#BaseHandler.load_middleware` |
| M02 | What does raising `MiddlewareNotUsed` do, and where is it handled? | `core/exceptions.py#MiddlewareNotUsed`, `core/handlers/base.py#BaseHandler.load_middleware` |
| M03 | How does a middleware declare it can run under async, and what happens when it can't? | `utils/deprecation.py#MiddlewareMixin`, `core/handlers/base.py#BaseHandler.adapt_method_mode` |
| M04 | A middleware `__call__` returns `None`. What happens? | `core/handlers/base.py#BaseHandler._get_response`, `#BaseHandler.check_response` |
| M05 | What is `convert_exception_to_response` for and where is it applied in the chain? | `core/handlers/exception.py#convert_exception_to_response`, `core/handlers/base.py#BaseHandler.load_middleware` |
| M06 | When is `process_view` called relative to URL resolution? | `core/handlers/base.py#BaseHandler._get_response`, `#BaseHandler.resolve_request` |
| M07 | How is `process_template_response` invoked and what must the response provide? | `core/handlers/base.py#BaseHandler._get_response` |
| M08 | How does `process_exception` differ from `convert_exception_to_response`? | `core/handlers/base.py#BaseHandler._get_response`, `core/handlers/exception.py#response_for_exception` |
| M09 | Which settings does `SecurityMiddleware` read, and where? | `middleware/security.py#SecurityMiddleware.__init__` |
| M10 | Under what conditions does `CommonMiddleware` issue a redirect? | `middleware/common.py#CommonMiddleware.process_request`, `#CommonMiddleware.should_redirect_with_slash` |
| M11 | What conditions cause `GZipMiddleware` to skip compression? | `middleware/gzip.py#GZipMiddleware.process_response` |
| M12 | What is the default `MIDDLEWARE` value shipped in global settings? | `conf/global_settings.py#MIDDLEWARE` |

### 4.3 Localization tasks (6) — DRAFT, UNVERIFIED

Answer = the address(es) you would edit. No prose required.

| ID | Task | Primary gold addresses (draft) |
|---|---|---|
| L01 | Add a hook that runs after middleware but before URL resolution | `core/handlers/base.py#BaseHandler._get_response` |
| L02 | Change how old-style `MiddlewareMixin` subclasses are adapted | `utils/deprecation.py#MiddlewareMixin` |
| L03 | Change what happens when a middleware raises during `load_middleware` | `core/handlers/base.py#BaseHandler.load_middleware` |
| L04 | Add a startup check that warns about middleware ordering | `core/checks/security/base.py` (module) |
| L05 | Add a new setting consumed by `SecurityMiddleware` | `middleware/security.py#SecurityMiddleware.__init__`, `conf/global_settings.py` |
| L06 | Where do the tests that assert middleware ordering live? | `tests/middleware_exceptions/tests.py`, `tests/handlers/tests.py` |

---

## 5. Scoring

### 5.1 Per-task score

```
address_score = ( |found ∩ required| + 0.5·|found ∩ supporting| ) / |required|
                clamped to [0, 1]

fact_score    = |facts asserted correctly| / |required_facts|

raw           = 0.5 · address_score + 0.5 · fact_score

penalty       = 0.25 × (number of forbidden_claims asserted confidently)

score_t       = max(0, raw − penalty)
```

Localization tasks have no `required_facts`; for those, `score_t = address_score − penalty`.

The penalty exists because a confidently wrong answer is worse than no answer — it
costs the caller a wasted turn *and* corrupts their model of the code. An arm that
hedges everything scores low on facts; an arm that bluffs scores low on penalty.

### 5.2 Matching rules — the fairness-critical part

**Address matching must not require agentlens's address syntax.** Arm B cannot emit
`file.py#Class.method`; grading it as though it should would rig the benchmark.

An address counts as found if the answer contains **either**:

- the symbolic form `path#Dotted` (path normalised, repo-relative, `/` separators), **or**
- a `path` plus a line or line range that **overlaps the gold symbol's span** in the
  pinned commit, **or**
- a `path` plus the bare symbol name appearing within 100 characters of it.

Span boundaries for rule 2 come from a gold-span table generated once at
verification time and frozen with the rest of the gold file.

**Fact matching** is `any_of` substring matching over a case-folded, whitespace-
collapsed answer, with a hand-written synonym list per fact. No LLM judge — judge
variance would swamp the effect being measured.

**Confidence detection** for penalties: a forbidden claim counts only if asserted
without hedging. Hedge markers (`might`, `possibly`, `I'm not sure`, `appears to`)
downgrade a forbidden claim to a 0.1 penalty instead of 0.25.

### 5.3 Aggregation

```
Accuracy      = mean(score_t) over 18 tasks
Cost          = Σ tool_result_tokens / Σ score_t          ← tokens per point
Navigation    = median over tasks of (tool calls until first gold address retrieved)
Head-to-head  = count of tasks where score_A > score_B, < , =
```

`Cost` deliberately divides by *points earned*, not tasks attempted. An arm that
saves tokens by answering nothing gets no credit.

---

## 6. Measurement rules

**Token accounting.** Count tokens in **tool results injected into context** — the
bytes the tool forces the caller to pay for. Counted with an external tokenizer, not
with agentlens's own `estimate_tokens`. Letting the tool score its own cost metric
would be indefensible.

Not counted: the model's reasoning tokens, the task prompt, the system prompt. These
are identical across arms and would dilute the effect.

**Caps.** Per task: 25 tool calls or 60,000 tool-result tokens, whichever first. On
cap, the task is scored on whatever the arm produced and flagged `capped: true`.
Capped tasks are reported separately as well as included in the totals — a cap is a
result, not a discard.

**Repetitions.** 3 independent runs per task per arm, fresh session each time, no
memory across tasks. Report median and interquartile range. Single-run agent
benchmarks are noise.

**Randomisation.** Task order shuffled per repetition with a recorded seed. Arm
order alternated.

**Determinism check.** agentlens output is deterministic by design; the harness
asserts byte-identical tool output across the 3 repetitions and flags any drift as a
tool bug rather than run variance.

---

## 7. Integrity controls

1. **Gold file frozen and hashed before any arm runs.** The blake3 hash is recorded
   in the results file. If gold changes after the run, the hash mismatch exposes it.
2. **Grading is mechanical.** A script, not a model, matches addresses and facts.
3. **The arms never see the gold file** — separate directory, not in the working tree.
4. **No agentlens self-measurement.** External tokenizer, external grader.
5. **Author bias, disclosed.** I built the tool. Mitigations: tasks derived from
   Django's own documentation sections rather than invented; gold answers cite a
   `source_of_truth` doc path; the falsification condition is pre-registered above;
   the omitted line-range baseline is named in §8 rather than buried.
6. **Raw transcripts retained** for every run so any number can be audited back to
   the tool calls that produced it.

---

## 8. Threats to validity — stated up front

| Threat | Effect on result |
|---|---|
| **Django is Python** | agentlens's only supported language, and its best case. The result does not generalise to Go, Rust, TS or Java. |
| **Middleware is symbol-shaped** | Classes and methods with clean boundaries — ideal for `slice`. A config-driven or metaprogramming-heavy feature would favour the tool much less. |
| **No line-range baseline** | Arm B is the weaker of the two realistic baselines. The measured advantage is therefore an **upper bound**, not a fair estimate. |
| **Baseline prompt quality** | How well Arm B is instructed to grep materially changes its cost. Arm B's prompt is written to be genuinely competent, and is included verbatim in the results for scrutiny. |
| **Whole-file size drives Arm B's cost** | `django/core/handlers/base.py` is large; a repo of small files would narrow the gap. File sizes touched are reported alongside the totals. |
| **Author-written gold** | See §7.5. |
| **Warm cache** | Arm A runs warm. Cold-start index build cost is reported as a footnote so the number can be adjusted. |

---

## 9. Run protocol

0. **Verification gate.** Clone Django at the pinned commit. Resolve every draft gold
   address. Any that fails to resolve is corrected or its task is cut. Generate the
   gold-span table. Freeze and hash the gold file. **No arm runs until this passes.**
1. Build the harness: arm runner, token counter, mechanical grader.
2. Warm the agentlens index once; record cold-build cost.
3. Execute 18 tasks × 2 arms × 3 repetitions = 108 runs.
4. Grade mechanically. Emit `results.json` + a summary table.
5. Report: three headline numbers, per-task table, head-to-head, capped tasks,
   threats section carried forward, and an explicit verdict against §1's
   falsification condition.

---

## 10. Open questions for you

1. **Pinned commit** — latest `main`, or a tagged release (e.g. 5.x) for
   reproducibility? Tag is the safer choice.
2. **Scale** — 108 runs is a real cost. Acceptable, or should I cut to 2
   repetitions (72 runs) or a 12-task set?
3. **The excluded arm** — add `rg` + line-range reads now as a third arm, or run
   two arms first and add it only if the win looks too easy?
4. **Cap sizes** — 25 calls / 60k tokens per task: too generous, too tight?
