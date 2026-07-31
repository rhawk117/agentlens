# AgentLens Django middleware benchmark

Run date: 2026-07-29

## Verdict

AgentLens passed the benchmark's pre-registered falsification test.

- Median cost ratio: **0.1706** (IQR 0.0442), meaning **5.86× fewer tool-result tokens per mechanical rubric point** than the `rg` + whole-file baseline.
- Median mechanical accuracy delta: **+0.3451** (IQR 0.0339).
- Per-task median head-to-head: **10 AgentLens wins, 0 losses, 8 ties**.
- Navigation: **3.5 calls** to first gold address for AgentLens versus **1.0 call** for baseline. Baseline's first broad `rg` commonly retrieved a gold address immediately, but injected far more text.
- Caps: **0 AgentLens**, **2 baseline**.

None of the three failure conditions fired:

| Pre-registered failure condition | Observed | Fired |
|---|---:|---:|
| Cost ratio ≥ 0.5 | 0.1706 | No |
| Accuracy >0.05 below baseline | AgentLens was 0.3451 higher | No |
| More than 4 outright task losses | 0 | No |

The accuracy number is a mechanical address/substring score, not a reliable estimate of semantic answer correctness. The fact matcher had severe false negatives; this limitation is material and discussed below.

## Headline metrics

Reported values are medians across three independent repetitions. Quartiles are calculated over those three repetition-level values.

| Metric | AgentLens | Baseline |
|---|---:|---:|
| Mechanical accuracy | 0.7231 (IQR 0.0046) | 0.3780 (IQR 0.0385) |
| Tool-result tokens per point | 4,187.9 (IQR 157.6) | 25,703.7 (IQR 6,964.4) |
| Calls to first gold address | 3.5 (IQR 0.5) | 1.0 (IQR 0.0) |
| Capped runs, all repetitions | 0 | 2 |

Across all 54 runs per arm, AgentLens consumed 165,272 tool-result tokens and earned 39.217 mechanical points. Baseline consumed 570,227 tokens and earned 21.742 points. These pooled totals are supporting context; the pre-registered headline uses repetition-level medians.

### Repetition results

| Repetition | AgentLens accuracy | Baseline accuracy | AgentLens tokens/point | Baseline tokens/point | Cost ratio |
|---:|---:|---:|---:|---:|---:|
| 1 | 0.7324 | 0.4535 | 4,187.9 | 20,148.5 | 0.2079 |
| 2 | 0.7231 | 0.3764 | 4,070.1 | 34,077.2 | 0.1194 |
| 3 | 0.7231 | 0.3780 | 4,385.4 | 25,703.7 | 0.1706 |

The two capped baseline runs were repetition 2 M04 and L01. They reached exactly 60,000 injected tool-result tokens and remained in the scored set.

## Per-task medians

`A` is AgentLens and `B` is baseline. Tokens are median injected tool-result tokens over three repetitions.

| Task | A score | B score | Result | A tokens | B tokens |
|---|---:|---:|---|---:|---:|
| M01 | 0.667 | 0.667 | Tie | 3,313 | 13,639 |
| M02 | 0.500 | 0.250 | A win | 1,482 | 7,063 |
| M03 | 0.600 | 0.100 | A win | 2,361 | 8,737 |
| M04 | 1.000 | 0.750 | A win | 2,072 | 44,314 |
| M05 | 0.500 | 0.500 | Tie | 1,152 | 3,871 |
| M06 | 0.500 | 0.000 | A win | 3,842 | 9,264 |
| M07 | 0.700 | 0.200 | A win | 3,578 | 7,258 |
| M08 | 0.500 | 0.100 | A win | 4,664 | 8,565 |
| M09 | 0.950 | 0.950 | Tie | 839 | 554 |
| M10 | 0.500 | 0.125 | A win | 2,487 | 2,151 |
| M11 | 0.500 | 0.000 | A win | 1,951 | 1,372 |
| M12 | 1.000 | 1.000 | Tie | 2,510 | 5 |
| L01 | 1.000 | 0.000 | A win | 4,537 | 44,634 |
| L02 | 1.000 | 1.000 | Tie | 4,254 | 8,430 |
| L03 | 1.000 | 1.000 | Tie | 1,055 | 2,697 |
| L04 | 1.000 | 1.000 | Tie | 1,918 | 2,755 |
| L05 | 0.000 | 0.000 | Tie | 9,472 | 18,502 |
| L06 | 1.000 | 0.000 | A win | 1,498 | 3,495 |

AgentLens was not cheaper on every task. Exact or simple literal lookups favored baseline on M09–M12, most dramatically M12, where one narrow `rg` result cost five tokens while AgentLens used a median 2,510.

## What to change in AgentLens

### 1. Make the CLI accept intuitive address forms

This is the clearest near-term improvement. Of 339 AgentLens research calls, **67 (19.8%) were syntax errors**, and another 12 calls/1,806 tokens were spent on help.

Forty-six failures were attempts to read a line range with intuitive forms such as:

- `slice path 1 120`
- `slice path:1-120`
- `slice path:L1-L120`

Other recurring mismatches were `map path#Symbol`, bare-file `slice` for an outline, `packet` with a concept or bare symbol, and path filters passed to `literals`.

Recommended change: accept those forms as aliases and route them to the canonical operation. Error output should include one correct invocation using the arguments the caller supplied.

### 2. Add exact identifier and module-constant lookup

M12 shows the gap. `literals MIDDLEWARE` did not behave like an identifier lookup, while `find MIDDLEWARE` returned roughly 2,383 tokens. A direct exact-symbol query for a module constant should return:

`django/conf/global_settings.py#MIDDLEWARE`

with its value and a canonical citation in one small result.

### 3. Emit canonical smallest edit-site addresses

L05 failed in both arms even though every AgentLens worker retrieved the correct implementation and defaults. Workers submitted broad addresses such as `#SecurityMiddleware` or the settings file rather than the frozen targets:

- `django/middleware/security.py#SecurityMiddleware.__init__`
- `django/conf/global_settings.py#L654-L665`

Add an edit-sites/localization mode that prefers the narrowest existing symbol plus a contiguous line anchor for module-level configuration blocks. Class results should prominently expose child method addresses.

### 4. Collapse large literal values in `map`

Each `map django/conf/global_settings.py` result cost 3,741 tokens because it expanded the large `LANGUAGES` literal before the relevant security settings. Collapse large values by default and offer filters such as:

`map django/conf/global_settings.py --match SECURE_`

Full literal expansion can remain opt-in.

### 5. Improve exact-definition ranking

`find CommonMiddleware` returned 1,217 tokens across definitions, tests, and references. Exact definitions should appear first in a compact section, with references/tests collapsed or separately requested. The same applies to class consumers and middleware setting names.

### 6. Support text/document files consistently

Workers tried `map` on documentation files and received errors. A lightweight outline/search view for text files would make cross-checking documented behavior cheaper and would reduce tool-switching.

### 7. Provide a compact command grammar in the agent prompt

The worker prompt named commands but did not include their argument grammar. This run partly measured CLI discovery. Future evaluations should provide a short, equal-quality syntax card for each arm.

## Retrieval quality versus grading quality

AgentLens retrieval was stronger than the reported absolute accuracy suggests:

- AgentLens answers cited **56 of 57 required comprehension addresses**.
- The only required comprehension address missed was `BaseHandler.resolve_request` in M06 repetition 3.
- The exact-phrase fact matcher recognized only **54 of 159 required facts** in AgentLens answers.

Examples of false negatives:

- M11 answers listed all four compression-skip conditions, but “shorter than 200 bytes” did not match the frozen phrase “less than 200 bytes.”
- M10 answers described both redirect paths and their conditions; two runs received zero of eight fact credits.
- M01 answers clearly said requests run top-to-bottom and responses unwind oppositely, but the frozen phrase alternatives missed that wording.
- M05 answers explained exception conversion, placement around each middleware, and the response guarantee, yet received zero fact credit.

The frozen grader was not changed after seeing results. A post-hoc sensitivity check that gives every comprehension answer full fact credit in both arms still produces per-repetition cost ratios of 0.239, 0.142, and 0.206, with AgentLens accuracy deltas of +0.273, +0.361, and +0.347. This does not repair the grader, but it suggests the benchmark verdict is not driven solely by the false negatives.

For a future benchmark, normalize Markdown punctuation and morphology at minimum. A blinded human or semantic adjudication layer with disagreement review would better measure answer correctness, while the frozen mechanical address score can remain as the reproducible localization metric.

## Integrity and reproducibility

### Pinned inputs

- Django: [6.0.7](https://docs.djangoproject.com/en/6.0/releases/6.0.7/)
  - tag object: `c7b3ee972494d7d9903db3db7cbe2e9ffe20f457`
  - commit: `e2a424605ac2e7e6e799496542fb2997207e2f23`
- AgentLens:
  - commit: `8033e6ccc00dcca316ab1e8ea83c5932296ec4a7`
  - version: `0.1.0`
  - release binary SHA-256: `81127fabf67018b9f83b17ffe1d3cdb7849f8258dd2d9f91767b08468127f69a`
- Frozen gold:
  - SHA-256: `ad9112eebd4a07daafff165d40c9b39c0b611eccc1fa45de4a892f48676ab39e`
  - BLAKE3: `45e7d1f546a491821f1f748f5266cd721c6c4d10d88f6d4f4f757241de216bf9`

### Execution controls

- 18 tasks × 2 arms × 3 repetitions = 108 runs.
- Every run used a unique fresh worker session with `fork_turns=none`.
- Task order was shuffled with recorded seeds; arm order alternated by repetition.
- Per-run caps were 25 tool calls and 60,000 injected result tokens.
- Token accounting used external `tiktoken` `o200k_base`.
- All prompts, task prompts, answers, transcripts, commits, binary, gold hashes, cap totals, and schedule events were revalidated after execution.
- AgentLens output was byte-identical for repeated identical invocations. Baseline `rg` occasionally emitted the same match set in a different traversal order; the validator compares its complete sorted line set.

The worker model name is attested by orchestration, not independently proven by a runtime model-ID receipt. Worker isolation was enforced by the prompt and wrapper interface, not by an OS-level sandbox that technically prevented direct file or web access.

### Warm-cache footnote

AgentLens ran with its index cache pre-warmed, as pre-registered. Cold index construction took 0.631 seconds, emitted 483 tool-result tokens, and produced an approximately 42 MiB cache. A warm repeat took 0.295 seconds and was byte-identical.

## Gold verification corrections

Before any scored run, every gold address was resolved against the pinned Django commit. The verified set corrected the draft as follows:

- Added the omitted `django/` prefix to source paths.
- Changed M04 from general middleware `__call__` returning `None` to a view or `process_template_response` hook returning `None`, matching the actual `check_response` behavior.
- Corrected M08 primary addresses to `process_exception_by_middleware` and `convert_exception_to_response`.
- Added `CommonMiddleware.process_response` to M10.
- Added the asynchronous handler path to L01.
- Replaced the incorrect L04/L06 draft locations with Django's actual `AuthenticationMiddleware`/`LoginRequiredMiddleware` startup-ordering check and tests.

The corrected structured gold was then frozen and hashed before run 1.

## Threats to validity

These results support the narrow claim that AgentLens was much more token-efficient than this `rg` + whole-file baseline on symbol-shaped Django middleware questions under the frozen mechanical rubric. They do not establish a general semantic-accuracy or cross-language claim.

Material limitations:

- The baseline had no line-range reads. The measured advantage is an upper bound; a disciplined `rg` + range-read arm should be the next comparison.
- Django is Python and middleware is unusually symbol-shaped, both favorable to AgentLens.
- AgentLens used a warm cache.
- The fact matcher materially understated semantic correctness.
- Only three repetitions were run.
- Baseline filesystem cache state was uncontrolled.
- The prompt named tools but omitted full command grammar.
- Runtime model identity and isolation are attested rather than independently enforced.

Because the cost ratio is below 0.25, the rubric itself says the next benchmark should add the omitted line-range baseline before this number is used externally.

## Repository health gate

The pinned AgentLens checkout built successfully, all 81 tests passed, and formatting passed. On Rust 1.97.1, `cargo clippy --all-targets --all-features -- -D warnings` failed with seven diagnostics across:

- `crates/agentlens-core/src/doc/mod.rs`
- `crates/agentlens-core/src/ops/find.rs`
- `crates/agentlens-core/src/ops/literals.rs`
- `crates/agentlens-core/src/ops/map.rs`
- `crates/agentlens-core/src/symbols.rs`

The diagnostics are cleanup lints (`use_self`, `unused_peekable`, `option_if_let_else`, and `or_fun_call`), not benchmark runtime failures.
