# agentlens v0.2.0 — Django middleware benchmark results

162 runs. 3 arms × 18 tasks × 3 repetitions, Django 6.0.7 at `e2a4246`, worker
model `claude-haiku-4-5-20251001`.

Method is specified in [`evals/METHODOLOGY.md`](evals/METHODOLOGY.md), which was
written before the campaign ran. This document reports numbers and does not
change any rule.

---

## Verdict

**agentlens passes every pre-registered falsification condition against both
controls.**

| Condition | Threshold | Measured | |
|---|---|---|---|
| Cost ratio vs Arm B (`rg` + `cat`) | < 0.5 | **0.4365** | pass |
| Accuracy vs Arm B | not > 0.05 below | **+0.0641** above | pass |
| Outright losses vs Arm B | ≤ 4 of 18 | **3** | pass |
| Cost ratio vs Arm C (`rg` + `sed`) | < 0.7 | **0.5145** | pass |
| Accuracy vs Arm C | not > 0.05 below | **+0.0451** above | pass |
| Outright losses vs Arm C | ≤ 6 of 18 | **2** | pass |

The grader reads these thresholds from `protocol.json` rather than from prose,
so the verdict cannot drift from the pre-registration.

**Two things a reader should weigh against that verdict before quoting it.**

1. The **accuracy** advantage is mostly an artefact of how localization answers
   are scored, and it nearly disappears when that artefact is removed
   ([§4](#4-the-localization-wording-bias)). The **cost** advantage survives.
2. The headline is roughly **half the advantage v0.1.0 reported** (0.4365 here
   against 0.1706 then). The campaigns are not comparable
   ([§6](#6-v010-vs-v020-not-a-comparison)) — but the widely quoted "5.9×
   cheaper" should not be carried forward.

The honest one-line summary: **on this task set agentlens answered the same
questions for about 2.3× fewer tokens than whole-file reading and about 1.9×
fewer than disciplined line-range reading, at approximately equal accuracy.**

---

## 1. Headline numbers

Medians across 3 repetitions, with the interquartile range.

| | **A — agentlens** | **B — baseline** (`rg`+`cat`) | **C — linerange** (`rg`+`sed`) |
|---|---|---|---|
| **Cost** (tool-result tokens per rubric point) | **5 088** (IQR 1 080) | 12 559 (IQR 6 214) | 6 455 (IQR 3 936) |
| **Accuracy** (0–1) | **0.6236** (IQR 0.0874) | 0.5595 (IQR 0.0439) | 0.5785 (IQR 0.0334) |
| **Navigation** (calls to first gold address) | 2.0 | **1.5** | **1.5** |
| Total tool-result tokens | **158 007** | 425 603 | 264 262 |
| Calls per run (median) | 7.0 | **3.5** | 8.5 |
| Capped runs | 2/54 | 1/54 | 2/54 |

**agentlens loses on navigation.** Both controls reach the first gold address in
a median 1.5 calls against agentlens's 2.0. `rg` goes straight from a plain-text
guess to a file and line; agentlens more often spends a call orienting
(`map`, `find`) before it can name an address. The tool wins on total tokens
while taking *more* calls to get its bearings, which is the opposite of the
"fewer, better-targeted calls" story it would be convenient to tell.

Note also that agentlens's accuracy IQR (0.0874) is two to three times either
control's. With only three repetitions that is a weak estimate, but the tool
under test was the least consistent arm across repetitions, not the most.

---

## 2. Per-task results

Median score across 3 repetitions. **Bold** marks the winning arm; a row with no
bold is a tie.

| Task | A | B | C | A tokens | B tokens | C tokens |
|---|---|---|---|---|---|---|
| M01 | 0.83 | 0.83 | 0.67 | 5 086 | 7 283 | 5 777 |
| M02 | 0.75 | 0.75 | 0.75 | 1 955 | 7 251 | 1 470 |
| M03 | **0.90** | 0.50 | 0.80 | 2 811 | 5 507 | 5 181 |
| M04 | **0.83** | 0.75 | 0.75 | 3 318 | 6 071 | 2 136 |
| M05 | **0.83** | 0.58 | 0.42 | 1 159 | 3 871 | 1 221 |
| M06 | 0.50 | **0.62** | 0.50 | 3 302 | 5 010 | 4 054 |
| M07 | 0.40 | **0.90** | **0.90** | 2 388 | 8 905 | 3 538 |
| M08 | **0.70** | 0.60 | 0.65 | 2 900 | 4 142 | 1 685 |
| M09 | 1.00 | 1.00 | 1.00 | 627 | 554 | 554 |
| M10 | **0.81** | 0.31 | 0.65 | 1 092 | 1 782 | 1 329 |
| M11 | 0.75 | **0.88** | **0.88** | 618 | 1 035 | 519 |
| M12 | **1.00** | **1.00** | 0.50 | 170 | 5 996 | 5 |
| L01 | 0.00 | 0.00 | 0.00 | 6 370 | 15 182 | 12 327 |
| L02 | 0.00 | 0.00 | 0.00 | 7 057 | 8 266 | 3 506 |
| L03 | **1.00** | 0.00 | 0.00 | 2 898 | 2 753 | 1 539 |
| L04 | 1.00 | 1.00 | 1.00 | 2 438 | 2 564 | 1 423 |
| L05 | 0.00 | 0.00 | 0.00 | 7 000 | 37 983 | 5 730 |
| L06 | 0.00 | 0.00 | 0.00 | 2 164 | 3 304 | 3 042 |

**Four tasks scored 0.00 for every arm** (L01, L02, L05, L06). That is not six
independent agent failures; it is one rubric problem, and §4 takes it apart.

**M07 is agentlens's worst task** (0.40 against 0.90 for both controls) and its
navigation median there is 26 calls — the cap. The tool failed to locate what
both controls found immediately.

---

## 3. Call quality: where agentlens spends calls the controls do not

A nonzero exit is not automatically a defect. `rg` exits 1 when it matches
nothing, and so does `agentlens find`; in both cases the arm asked a well-formed
question and learned something true. Those are separated here from *usage*
failures, where the arm did not know how to phrase the call and the budget paid
for nothing.

| | agentlens | baseline | linerange |
|---|---|---|---|
| Total calls | 513 | 226 | 474 |
| **Usage errors** | **39 (7.6%)** | **0 (0.0%)** | **0 (0.0%)** |
| Empty results | 84 (16.4%) | 13 (5.8%) | 25 (5.3%) |

**One in thirteen agentlens calls was malformed. Neither control produced a
single malformed call.** `rg` and `sed` have interfaces the model already knows;
agentlens's it does not. The failures are concentrated in invented flags
(`--expand`, `--include-strings`, `-t`) and in passing an outline or line span
where a symbol address is required:

```
6  error: unexpected argument '--expand' found
4  callers needs a symbol address, not an outline or a line span
3  `process_template_response` is ambiguous: 5 definitions
3  packet needs a symbol address, not an outline or a line span
2  error: unexpected argument '--include-strings' found
2  error: unexpected argument '-t' found
```

This is the benchmark's clearest actionable finding, and it is a finding against
the tool. agentlens wins on tokens *despite* wasting 7.6% of its calls on syntax
it got wrong — but that also means the measured advantage is a floor, not a
ceiling, for a version whose interface a model could use correctly. The
ambiguity failures are the most interesting: the tool knows all five definitions
and refuses to pick, spending a call to say so.

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
| agentlens | 0.444 | **1.000** |
| baseline | 0.167 | **1.000** |
| linerange | 0.194 | **0.944** |

**Every arm located the correct file essentially every time.** The measured
localization gap was notation, not navigation.

### Effect on the headline

Recomputed from the same transcripts, changing only the localization address
rule and leaving all 12 comprehension tasks at their pre-registered scores:

| | Pre-registered (authoritative) | Debiased |
|---|---|---|
| agentlens accuracy | 0.6236 | 0.8319 |
| baseline accuracy | 0.5595 | 0.8097 |
| linerange accuracy | 0.5785 | 0.8082 |
| Accuracy delta vs B | +0.0641 | **+0.0221** |
| Accuracy delta vs C | +0.0451 | **+0.0237** |
| Cost ratio vs B | 0.4365 | **0.3614** |
| Cost ratio vs C | 0.5145 | **0.5809** |

Two conclusions, and they point in opposite directions:

- **The accuracy claim is weak.** Most of the pre-registered advantage was the
  wording artefact. Debiased, all three arms sit within 0.024 of each other —
  which is *approximate parity*, not a win. The correct claim is "without losing
  accuracy", which is what METHODOLOGY §1 actually asserts.
- **The cost claim is robust.** It passes both thresholds under either rule, and
  the direction of the change differs by control (better against B, worse
  against C), which is what an artefact rather than a thumb on the scale looks
  like.

The pre-registered numbers remain authoritative for the verdict. This
sensitivity analysis is reported because publishing only the favourable framing
of a rubric the author wrote would be indefensible; it is reproducible via
`evals/harness/sensitivity.py`.

**For v3:** the localization prompt must state the required granularity in
tool-neutral words — *"name the file and the function or class you would
edit"* — and the gold must accept file-level answers at partial credit. Four
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
| Memorisation under cap | 2 |
| Quarantined / re-run | 0 |

The two flagged runs — `r1-agentlens-L05` and `r3-baseline-L01` — cite a file
absent from their own transcript, but both hit the call cap, and the prompt then
tells a capped worker to submit its best available answer. They are retained and
reported rather than re-rolled: re-running a capped run in the author's own arm
is indistinguishable from re-rolling until the tool looks better. One is in
Arm A and one in Arm B, so the effect does not favour either.

`validate_protocol.py` independently confirms, for all 162 runs: prompt
rendering matches byte-for-byte, prompt/answer/transcript SHA-256 hashes match
the attestation log, no run exceeded 25 calls or 60 000 tokens, identical tool
invocations produced identical output across repetitions, gold BLAKE3 is
unchanged, and the binary hash matches the pinned build.

---

## 6. v0.1.0 vs v0.2.0: not a comparison

Both campaigns re-graded with the identical v2 matcher:

| | v0.1.0 | v0.2.0 |
|---|---|---|
| Worker model | `gpt-5.6-sol` | `claude-haiku-4-5-20251001` |
| agentlens accuracy | 0.8627 (IQR 0.012) | 0.6236 (IQR 0.087) |
| baseline accuracy | 0.5199 | 0.5595 |
| Cost ratio vs baseline | 0.1938 | 0.4365 |
| agentlens navigation | 3.5 | 2.0 |

**These numbers must not be read as a v0.1.0 → v0.2.0 regression.** The worker
model changed *family*, not just version. Every cross-campaign delta confounds
tool version with model, and the confound is almost certainly dominant: the
baseline arm — whose tools did not change at all — also moved (0.5199 → 0.5595),
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

**The campaign ran 3 repetitions, not the pre-registered 5.** It was stopped
after repetition 3 by the operator on cost grounds — *"No more repitions this is
way too expensive"* — before any run had been graded. No score, cost ratio or
verdict existed at that moment. `schedule.json` still carries the unexecuted
repetitions 4 and 5 rather than being regenerated to match, and `protocol.json`
records `repetitions_scheduled: 5` alongside `repetitions_executed: 3`.

The price is statistical and it is real: every interval here rests on three
points, so the IQRs are crude and one anomalous repetition moves a median more
than it should. agentlens's own accuracy ranged 0.571 / 0.624 / 0.746 across the
three — a spread wide enough that 5 repetitions could have shifted the reported
median noticeably.

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
| **Localization prompt wording** | **New, confirmed.** See §4. Four of 18 tasks currently measure nothing. |
| **agentlens interface is unfamiliar to the model** | **New, quantified.** 7.6% of its calls were malformed against 0% for both controls (§3). |
| **Baseline prompt quality** | Both control prompts are published verbatim and were written to be competent. Arm C's exclusion of `cat` is deliberate and stated. |
| **Whole-file size drives Arm B's cost** | Confirmed — L05 cost Arm B 37 983 tokens. A repo of small files narrows the gap. |
| **Warm cache** | Not a threat. Only `callers` and `dead` touch the index, and cold and warm output are byte-identical, so cache state cannot move token cost. Cold build: 1.019 s, 100 tool-result tokens, ~43 MB. |
| **Matcher changed between campaigns** | Both campaigns re-graded under matcher v2. v1 scores are not interchangeable with these. |
| **Worker model pinned but unproven** | Unchanged. Dispatch configuration is recorded; a subagent cannot emit a runtime receipt. |
| **3 repetitions, not 5** | §7. |
| **Cross-campaign model change** | §6. v0.1.0 comparison is uninterpretable as a tool-version delta. |

---

## 9. Pinned inputs

| | |
|---|---|
| Subject | `django/django` @ `e2a424605ac2e7e6e799496542fb2997207e2f23` (tag 6.0.7) |
| Tool | `agentlens` 0.2.0 @ `61ba0258821dcce46f7aa99617385fa88752eb4c` |
| Binary SHA-256 | `77f8981529daafd1bd2263cc0adc1dd36f91d055cd819bbe6c9a2faaecdff5b4` |
| Worker model | `claude-haiku-4-5-20251001` (attested by dispatch, not proven) |
| Gold SHA-256 | `ad9112eebd4a07daafff165d40c9b39c0b611eccc1fa45de4a892f48676ab39e` |
| Gold BLAKE3 | `45e7d1f546a491821f1f748f5266cd721c6c4d10d88f6d4f4f757241de216bf9` |
| Schedule base seed | `20260730` |
| Tokenizer | `tiktoken` `o200k_base` 0.8.0 |
| Caps | 25 calls / 60 000 tool-result tokens per task |

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
it.
