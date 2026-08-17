# agentlens v0.2.0 — Django middleware benchmark results

162 runs. 3 arms × 18 tasks × 3 repetitions, Django 6.0.7 at `e2a4246`, worker
model `claude-haiku-4-5-20251001`.

Method is specified in [`evals/METHODOLOGY.md`](evals/METHODOLOGY.md), which was
written before the campaign ran. This document reports numbers and does not
change any rule.

**This document reports the 2026-08-17 re-run and replaces, in full, the
numbers previously published from the discarded 2026-07-30 execution.** That
execution was withdrawn because the binary under test was rebuilt mid-campaign
and its runs cannot be attested to a single build ([§7](#7-deviations-from-the-pre-registration)).
The verdict direction is unchanged — both executions passed every pre-registered
condition — but every number moved, and only the numbers below should be quoted.

---

## Verdict

**agentlens passes every pre-registered falsification condition against both
controls.**

| Condition | Threshold | Measured | |
|---|---|---|---|
| Cost ratio vs Arm B (`rg` + `cat`) | < 0.5 | **0.4205** | pass |
| Accuracy vs Arm B | not > 0.05 below | **+0.0331** above | pass |
| Outright losses vs Arm B | ≤ 4 of 18 | **3** | pass |
| Cost ratio vs Arm C (`rg` + `sed`) | < 0.7 | **0.6095** | pass |
| Accuracy vs Arm C | not > 0.05 below | **−0.0155** below | pass |
| Outright losses vs Arm C | ≤ 6 of 18 | **2** | pass |

The grader reads these thresholds from `protocol.json` rather than from prose,
so the verdict cannot drift from the pre-registration.

**Three things a reader should weigh against that verdict before quoting it.**

1. **agentlens is not the most accurate arm.** The disciplined line-range
   control beat it on median accuracy (0.6833 against 0.6044). The margin,
   −0.0155 per repetition, is inside the pre-registered tolerance, and the
   claim under test was "without losing accuracy", not "more accurate" — but
   the direction should be stated, not buried.
2. The **accuracy** picture changes further when the localization wording
   artefact is removed ([§4](#4-the-localization-wording-bias)): debiased, all
   three arms sit within 0.025 of each other, with agentlens nominally last.
   The **cost** advantage survives debiasing against both controls, though
   against Arm C it survives by very little (0.6942 against a 0.7 threshold).
3. Against Arm C the win is **cost, narrowly** — 0.6095 against a 0.7
   threshold on the pre-registered rule — not the comfortable 2.4× margin the
   Arm B comparison suggests. Quoting only the Arm B number would be
   selecting the kinder control.

The honest one-line summary: **on this task set agentlens answered the same
questions for about 2.4× fewer tokens per rubric point than whole-file reading
and about 1.6× fewer than disciplined line-range reading, at approximately
equal accuracy.**

---

## 1. Headline numbers

Medians across 3 repetitions, with the interquartile range.

| | **A — agentlens** | **B — baseline** (`rg`+`cat`) | **C — linerange** (`rg`+`sed`) |
|---|---|---|---|
| **Cost** (tool-result tokens per rubric point) | **7 511** (IQR 1 329) | 15 523 (IQR 3 707) | 8 257 (IQR 3 067) |
| **Accuracy** (0–1) | 0.6044 (IQR 0.0938) | 0.5875 (IQR 0.0473) | **0.6833** (IQR 0.0343) |
| **Navigation** (calls to first gold address) | 2.0 | **1.5** | 2.0 |
| Total tool-result tokens | **225 618** | 480 372 | 329 790 |
| Calls per run (median) | 12.0 | **4.0** | 12.0 |
| Capped runs | 7/54 | 1/54 | 8/54 |

**agentlens loses on navigation and on accuracy, and wins on cost.** Arm B
reaches the first gold address in a median 1.5 calls — `rg` goes straight from
a plain-text guess to a file and line — while agentlens more often spends a
call orienting (`map`, `find`) before it can name an address. Arm C is the
most accurate arm outright. The tool's advantage is that each of its calls
buys far fewer tokens: it takes as many calls as Arm C and three times Arm B's,
and still finishes at roughly half of either control's total token bill.

Note also that agentlens's accuracy IQR (0.0938) is two to three times either
control's, and its per-repetition accuracy fell monotonically across the
campaign (0.7417 / 0.6044 / 0.5542). With three repetitions that ordering is
weak evidence of anything, but the tool under test was again the least
consistent arm, not the most.

Capped runs also tripled or better against the discarded execution's counts in
the two precise-read arms (A 7/54, C 8/54 against B's 1/54): the arms that pay
per span spend more calls, and more of them run out.

---

## 2. Per-task results

Median score across 3 repetitions. **Bold** marks the winning arm; a row with no
bold is a tie.

| Task | A | B | C | A tokens | B tokens | C tokens |
|---|---|---|---|---|---|---|
| M01 | 0.83 | 0.83 | 0.83 | 6 246 | 14 400 | 7 469 |
| M02 | **0.75** | 0.58 | **0.75** | 1 969 | 7 251 | 2 356 |
| M03 | **0.90** | **0.90** | 0.78 | 2 487 | 7 154 | 6 249 |
| M04 | **0.83** | 0.75 | 0.75 | 2 930 | 6 335 | 2 829 |
| M05 | **0.75** | 0.58 | 0.42 | 1 166 | 3 871 | 1 112 |
| M06 | 0.50 | **0.62** | 0.50 | 4 235 | 9 264 | 10 775 |
| M07 | 0.80 | **0.90** | 0.80 | 6 168 | 7 258 | 2 921 |
| M08 | **0.70** | **0.70** | 0.60 | 3 196 | 6 242 | 3 440 |
| M09 | 1.00 | 1.00 | 1.00 | 627 | 631 | 1 195 |
| M10 | **0.81** | 0.50 | 0.58 | 3 855 | 3 057 | 2 295 |
| M11 | 0.88 | 0.88 | 0.88 | 643 | 1 035 | 1 382 |
| M12 | 0.50 | **1.00** | **1.00** | 2 177 | 6 040 | 48 |
| L01 | 0.00 | 0.00 | 0.00 | 5 980 | 11 950 | 9 321 |
| L02 | 0.00 | 0.00 | 0.00 | 10 206 | 19 377 | 8 493 |
| L03 | **1.00** | 0.00 | **1.00** | 4 791 | 6 940 | 1 868 |
| L04 | 1.00 | 1.00 | 1.00 | 5 403 | 7 054 | 5 371 |
| L05 | 0.00 | 0.00 | 0.00 | 6 516 | 11 492 | 3 857 |
| L06 | 0.00 | 0.00 | **1.00** | 3 219 | 5 101 | 856 |

**Three tasks scored 0.00 for every arm** (L01, L02, L05). That is not nine
independent agent failures; it is one rubric problem, and §4 takes it apart.
L06 — all-zero in the discarded execution — was solved this time by Arm C
alone, in a median 856 tokens.

**M12 is agentlens's worst task** (0.50 against 1.00 for both controls), and
the token column is the embarrassment: Arm C answered it from a median of 48
result tokens, meaning one precise read was enough for a task agentlens spent
2 177 tokens on and still half-failed.

---

## 3. Call quality: where agentlens spends calls the controls do not

A nonzero exit is not automatically a defect. `rg` exits 1 when it matches
nothing, and so does `agentlens find`; in both cases the arm asked a well-formed
question and learned something true. Those are separated here from *usage*
failures, where the arm did not know how to phrase the call and the budget paid
for nothing.

| | agentlens | baseline | linerange |
|---|---|---|---|
| Total calls | 696 | 286 | 683 |
| **Usage errors** | **44 (6.3%)** | 3 (1.0%) | 12 (1.8%) |
| Empty results | 78 (11.2%) | 13 (4.5%) | 46 (6.7%) |

**One in sixteen agentlens calls was malformed, against roughly one in a
hundred for Arm B.** The controls are no longer at zero — their failures are
almost all `rg` invoked with a bare symbol name where a path argument was
expected (`rg: AuthenticationMiddleware: No such file or directory`) — but the
gap remains wide, and it remains a finding against the tool. agentlens's
failures concentrate in flags it does not have and in passing an outline or
line span where a symbol address is required:

```
4  packet needs a symbol address, not an outline or a line span
4  error: unexpected argument '-t' found
3  error: unexpected argument '-n' found
3  error: unexpected argument '-L' found
3  agentlens: cannot read `django/core/handlers/base.py BaseHandler.load_…`
2  error: unexpected argument '--expand' found
```

`rg` and `sed` have interfaces the model already knows; agentlens's it does
not. The tool wins on tokens *despite* wasting 6.3% of its calls on syntax it
got wrong — the measured advantage is a floor, not a ceiling, for a version
whose interface a model could use correctly. The invented flags (`-t`, `-n`,
`-L`) are `rg`'s flags: the model is pattern-matching agentlens onto the tool
it already knows.

---

## 4. The localization wording bias

**This is the largest correction in the report, and it runs against the tool
under test.**

Six of the eighteen tasks are localization. The worker prompt instructs:

> For localization tasks, return only the repo-relative address(es) you would edit.

*Address* is **agentlens's own vocabulary**. Its output is `path#Symbol`, so its
answers carry symbol granularity for free. A control arm reasonably reads the
same word as "path" and answers `django/core/handlers/base.py` — which the gold
scores zero, because the gold names
`django/core/handlers/base.py#BaseHandler._get_response`.

The rubric explicitly forbids exactly this: *"Address matching must not require
agentlens's address syntax"* (METHODOLOGY §5.2). The address *matcher* obeys that
rule — it accepts prose line citations, bare paths near a symbol name, and
several other forms. The **prompt** does not. The bias lives in the question, not
the grader, which is why fixing the matcher in v0.2.0 did not catch it.

Re-scoring localization so that naming the correct **file** counts as found:

| Localization address recall | Pre-registered | File-only |
|---|---|---|
| agentlens | 0.417 | **0.944** |
| baseline | 0.194 | **0.972** |
| linerange | 0.444 | **0.917** |

**Every arm located the correct file essentially every time.** The measured
localization gap was notation, not navigation.

### Effect on the headline

Recomputed from the same transcripts, changing only the localization address
rule and leaving all 12 comprehension tasks at their pre-registered scores:

| | Pre-registered (authoritative) | Debiased |
|---|---|---|
| agentlens accuracy | 0.6044 | 0.8093 |
| baseline accuracy | 0.5875 | 0.8340 |
| linerange accuracy | 0.6833 | 0.8213 |
| Accuracy delta vs B | +0.0331 | **−0.0247** |
| Accuracy delta vs C | −0.0155 | **−0.0120** |
| Cost ratio vs B | 0.4205 | **0.4840** |
| Cost ratio vs C | 0.6095 | **0.6942** |

Two conclusions, and they point in opposite directions:

- **The accuracy claim is weak, and weaker than last reported.** Debiased,
  agentlens is nominally the *least* accurate of the three arms, though all
  three sit within 0.025 — approximate parity. The pre-registered +0.0331
  against Arm B is mostly the wording artefact. The defensible claim is
  "without losing accuracy beyond the pre-registered margin", and nothing
  stronger.
- **The cost claim survives debiasing, but against Arm C it survives by
  0.0058.** Under the debiased rule the Arm C cost ratio is 0.6942 against
  the 0.7 falsification line. The pre-registered rule is authoritative and
  passes with more room (0.6095), but anyone treating the debiased numbers as
  the truer reading should know how close that margin is.

The pre-registered numbers remain authoritative for the verdict. This
sensitivity analysis is reported because publishing only the favourable framing
of a rubric the author wrote would be indefensible; it is reproducible via
`evals/harness/sensitivity.py`.

**For v3:** the localization prompt must state the required granularity in
tool-neutral words — *"name the file and the function or class you would
edit"* — and the gold must accept file-level answers at partial credit. Three
tasks currently measure nothing.

---

## 5. Isolation and data integrity

Every arm reached the subject only through `bench_tool.py`, enforced by a
PreToolUse hook denying `Bash`, `Read`, `Grep` and `Glob` calls that name the
subject or the answer key, and verified after the fact against the transcripts.

| | Count |
|---|---|
| Runs checked | 162 |
| **Isolation breaches** | **0** |
| Memorisation under cap | 1 |
| Quarantined / re-run | 0 |

The one flagged run — `r2-linerange-L01`, a control-arm run — cites
`django/core/handlers/wsgi.py` without having retrieved it, but it hit the
call cap, and the prompt then tells a capped worker to submit its best
available answer. It is retained and reported rather than re-rolled, per the
methodology's standing rule; the flag is in a control arm, so retaining it
does not favour the tool.

`validate_protocol.py` independently confirms, for all 162 runs: prompt
rendering matches byte-for-byte, prompt/answer/transcript SHA-256 hashes match
the attestation log, no run exceeded 25 calls or 60 000 tokens, identical tool
invocations produced identical output across repetitions, gold BLAKE3 is
unchanged, and the binary hash matches the pinned build. For this campaign the
binary claim is meaningful in a way it was not for the discarded execution:
the SHA-256 was verified against `protocol.json` before the first worker was
dispatched, and the binary was not rebuilt during the run.

---

## 6. v0.1.0 vs v0.2.0: not a comparison

Both campaigns re-graded with the identical v2 matcher:

| | v0.1.0 | v0.2.0 (this campaign) |
|---|---|---|
| Worker model | `gpt-5.6-sol` | `claude-haiku-4-5-20251001` |
| agentlens accuracy | 0.8627 (IQR 0.012) | 0.6044 (IQR 0.094) |
| baseline accuracy | 0.5199 | 0.5875 |
| Cost ratio vs baseline | 0.1938 | 0.4205 |
| agentlens navigation | 3.5 | 2.0 |

**These numbers must not be read as a v0.1.0 → v0.2.0 regression.** The worker
model changed *family*, not just version. Every cross-campaign delta confounds
tool version with model, and the confound is almost certainly dominant: the
baseline arm — whose tools did not change at all — also moved (0.5199 → 0.5875),
and §3 shows agentlens's interface is the one that costs a model calls to use
correctly. A model less fluent in that interface depresses Arm A specifically,
which is exactly the pattern observed.

What can be said: **within v0.2.0 all three arms ran the same model, the same
prompts and the same task set, so the arm comparison in §1 is sound.** The
cross-campaign row is published for traceability, not for inference.

The v0.1.0 environment leak (`RIPGREP_CONFIG_PATH` reaching measured commands
and billing its warning to a control arm) is a second, independent reason its
cost figures ran favourable to agentlens. v0.2.0 uses an environment allowlist.

---

## 7. Deviations from the pre-registration

**1. The first v0.2.0 execution was discarded after publication and the
campaign re-run on 2026-08-17.** The wrapper resolves the binary under test
from `target/release/agentlens`, and that binary was rebuilt at 23:56 on
2026-07-30 — inside the campaign's 23:42–00:34 execution window. Runs on
either side of that instant cannot be attested to the same build, so the
execution and its published numbers were withdrawn in full. The discard
decision was made on provenance evidence alone, before any re-run number
existed. The discarded corpus is archived at
`.eval/archive-v2-attempt1-mixed-binary/` for audit. For the re-run, the
binary's SHA-256 was verified against the `protocol.json` pin before dispatch,
and the subject checkout was re-cloned and re-verified against the pinned
commit (`verify_gold.py` passing on all 18 tasks).

**2. The campaign ran 3 repetitions, not the pre-registered 5** (carried over
from the original execution's protocol). The cut was made on 2026-07-30 by the
operator on cost grounds — *"No more repitions this is way too expensive"* —
before any run had been graded. `schedule.json` still carries the unexecuted
repetitions 4 and 5, and `protocol.json` records `repetitions_scheduled: 5`
alongside `repetitions_executed: 3`. The price is statistical and it is real:
every interval here rests on three points. agentlens's own accuracy ranged
0.7417 / 0.6044 / 0.5542 across the three — a spread wide enough that 5
repetitions could have shifted the reported median noticeably.

**3. One mid-campaign pause, and 19 runs needed a second worker session.**
The operator paused the re-run once, to verify the worker model identity
(verified: every API message in every dispatch transcript records
`claude-haiku-4-5-20251001`). 14 runs stranded by that pause, and 5 runs whose
worker composed an answer but never invoked `submit`, were each re-dispatched
as a fresh session continuing the *same* run: the wrapper's transcript
persists, so the second session inherited the first's remaining call and token
budget, and `submit` remains once-only. No answer was discarded or re-rolled;
the affected runs are ordinary members of the sample.

No other deviation. Task set, gold, caps, thresholds and arm definitions are as
pre-registered.

---

## 8. Threats to validity

Carried from METHODOLOGY §8, with what this campaign showed.

| Threat | Status after v0.2.0 |
|---|---|
| **Django is Python** | Unchanged. agentlens's only supported language and its best case. Nothing here generalises to Go, Rust, TypeScript or Java. |
| **Middleware is symbol-shaped** | Unchanged, and now visibly load-bearing: the whole design advantage is symbol addressing, and this feature area is made of clean symbols. |
| **Author-written gold** | §4 found a real author-introduced bias favouring the tool. It was found and disclosed, but it was found *after* the run — treat remaining gold as carrying similar unfound risk. |
| **Localization prompt wording** | **Confirmed again.** See §4. Three of 18 tasks currently measure nothing. |
| **agentlens interface is unfamiliar to the model** | **Quantified again.** 6.3% of its calls were malformed against 1.0% and 1.8% for the controls (§3), and the invented flags are `rg`'s. |
| **Baseline prompt quality** | Both control prompts are published verbatim and were written to be competent. Arm C's exclusion of `cat` is deliberate and stated. |
| **Whole-file size drives Arm B's cost** | Confirmed — L02 cost Arm B a median 19 377 tokens per run. A repo of small files narrows the gap. |
| **Warm cache** | Not a threat. Only `callers` and `dead` touch the index, and cold and warm output are byte-identical, so cache state cannot move token cost. Cold build: 1.019 s, 100 tool-result tokens, ~43 MB. |
| **Matcher changed between campaigns** | Both campaigns re-graded under matcher v2. v1 scores are not interchangeable with these. |
| **Worker model pinned but unproven** | Softened one step for this campaign: dispatch-side transcripts record the model identifier on every API message, all reading Haiku 4.5. Still a dispatch-side record, not a runtime receipt from the worker. |
| **Re-run after publication** | **New.** This campaign replaced numbers that had already been published. The discard was provenance-driven and decided before any re-run number existed, but a reader cannot verify intent — the ordering evidence (rebuild mtime inside the run window, archived corpus, unchanged protocol) is what is offered. |
| **3 repetitions, not 5** | §7. |
| **Cross-campaign model change** | §6. v0.1.0 comparison is uninterpretable as a tool-version delta. |

---

## 9. Pinned inputs

| | |
|---|---|
| Subject | `django/django` @ `e2a424605ac2e7e6e799496542fb2997207e2f23` (tag 6.0.7) |
| Tool | `agentlens` 0.2.0 @ `61ba0258821dcce46f7aa99617385fa88752eb4c` |
| Binary SHA-256 | `77f8981529daafd1bd2263cc0adc1dd36f91d055cd819bbe6c9a2faaecdff5b4` (verified before dispatch) |
| Worker model | `claude-haiku-4-5-20251001` (recorded on every dispatch-transcript message) |
| Gold SHA-256 | `ad9112eebd4a07daafff165d40c9b39c0b611eccc1fa45de4a892f48676ab39e` |
| Gold BLAKE3 | `45e7d1f546a491821f1f748f5266cd721c6c4d10d88f6d4f4f757241de216bf9` |
| Schedule base seed | `20260730` |
| Tokenizer | `tiktoken` `o200k_base` 0.8.0 |
| Caps | 25 calls / 60 000 tool-result tokens per task |
| Campaign executed | 2026-08-17 (re-run; first execution discarded, §7) |

## 10. Reproducing

```bash
uv sync --directory evals/harness

uv run --directory evals/harness python verify_gold.py        # gold resolves
uv run --directory evals/harness python validate_protocol.py  # 162 attestations
uv run --directory evals/harness python detect_leaks.py       # isolation
uv run --directory evals/harness python grade.py              # §1, §2
uv run --directory evals/harness python stats.py              # §3
uv run --directory evals/harness python sensitivity.py        # §4
```

Full results in `evals/harness/results.json`; the v0.1.0 re-grade in
`evals/harness/results_v1_regraded.json`. Raw transcripts for all 162 runs are
retained, so any number here can be audited back to the tool calls that produced
it. The discarded first execution's runs, attestations and grades are archived
under `.eval/archive-v2-attempt1-mixed-binary/`.
