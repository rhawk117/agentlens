# Install Script, Agent Kit, and README — Design

**Date:** 2026-07-29
**Branch:** `chore/plan-alpha`
**Repo:** `rhawk117/agentlens`

## Goal

Three deliverables, one PR into `dev`:

1. A release pipeline and an install script that puts `agentlens` and `doclens`
   into `~/.local/bin`, preferring a published release binary and falling back
   to a source build.
2. A skill and a subagent for driving the tool, deployable to both Claude Code
   and GitHub Copilot from a single authored source.
3. A README rewrite with CI badges and alert blocks.

## Decisions

Each of these was settled by the user during brainstorming. The rejected options
are recorded because without them a later reader reopens the question.

### Release pipeline is in scope

No releases exist today. The install script's download path would be dead code
without a pipeline to produce assets, so `release.yml` ships alongside it.

Rejected: install script only (download path stays unexercised); source-build
only (defers the problem without shrinking it).

### Four native targets, no cross-compilation

`x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`,
`x86_64-pc-windows-msvc` — each on its native GitHub runner.

Rejected: adding `aarch64-unknown-linux-gnu` and a musl static build. Both need
`cross` or a custom linker setup, and musl combined with tree-sitter's C
compilation is the most common failure point in this class of pipeline. Linux
ARM users take the source-build path, which the script handles as a normal
outcome rather than an error.

Rejected: dropping Windows assets. The CI matrix already proves Windows builds.

### Agent kit authored once, deployed by script

Canonical source lives in `agent-kit/`. `install-kit.sh` assembles and copies it
into both toolchains' layouts.

Rejected: committing both native layouts in-repo. Discoverable without running
anything, but it duplicates the agent body and the two copies will drift.

Rejected: Claude-native plus an `AGENTS.md` pointer for Copilot. Honest about
Copilot's limits but gives Copilot users materially less.

### Copilot agent variant drops the model pin

Identical prompt and response contract. The Claude Code variant sets
`model: claude-haiku-4.5`; the Copilot variant omits `model` entirely.

Rejected: pinning the nearest model Copilot exposes — it may be ignored or
rejected, and a config that silently does nothing is worse than an absent one.
The value here is the capped-response contract, not the model selection.

### Agent returns XML-enveloped markdown

Measured with `tiktoken` on identical payloads across four serializations:

| Format | Bytes | Lines | o200k_base | cl100k_base | Ratio |
|---|---|---|---|---|---|
| Markdown | 748 | 34 | 209 | 216 | 1.00 |
| **XML + markdown** | **847** | **39** | **241** | **248** | **1.15** |
| YAML | 1080 | 40 | 299 | 303 | 1.43 |
| XML | 1330 | 35 | 372 | 376 | 1.78 |
| JSON | 1318 | 42 | 379 | 383 | 1.81 |

Two independent encodings agree to within 3%, so the ordering holds even though
neither is Claude's own tokenizer.

The envelope's tags cost 55 tokens measured alone, but add only 32 over plain
markdown, because they replace markdown's own `##` headings and `---` separator.
That fixed cost is roughly 11 tokens per question and does not grow with the
answer, so on a report carrying real slices the ratio approaches 1.02.

Chosen over plain markdown for three reasons:

- `status` is an attribute rather than prose, so it cannot be misread.
- Evidence containing `##` (likely, since `doclens` slices markdown) collides
  with plain markdown's own structure. Tags do not.
- A capped response truncated mid-report leaves an unclosed `<q>`. In plain
  markdown, truncated and complete output look identical — and capping is
  precisely the condition that produces truncation.

Rejected: plain markdown (209 tokens, the floor, but no truncation signal);
YAML (parseable and cheaper than JSON, but block scalars for code snippets are
easy to emit malformed); strict JSON and full XML (both ~1.8x for no gain, since
the consumer is always a model and never a parser).

### One PR

All three deliverables are interdependent: the install script offers the agent
kit, and the README documents both.

## Architecture

### Asset contract

The coupling point between the pipeline and the installer. Fixed, and documented
in both files so a change to one is visibly a change to the other.

```
agentlens-<tag>-<target>.tar.gz     # .zip for windows-msvc
SHA256SUMS
```

Each archive contains both binaries plus `LICENSE` and `README.md`. They share a
core crate and the skill teaches both, so splitting them would mean two
downloads to make the skill work.

`SHA256SUMS` is generated in a single final job that collects every matrix
artifact, so all hashes come from one tool on one platform.

### `install.sh`

POSIX `sh` at the repo root.

1. Detect target from `uname -s` and `uname -m`.
2. Resolve version: `--version`, else the GitHub API's `releases/latest`.
3. A 404 from that endpoint means no releases are published. This is the
   expected state today and is **not an error** — report it and fall through to
   the source build. Same when the detected target has no matching asset.
4. Download archive and `SHA256SUMS`, verify with `sha256sum` or `shasum -a 256`,
   extract, install both binaries into the target directory.
5. Source fallback requires `cargo`. In-repo uses `--path`; outside uses `--git`.
   Absent `cargo`, exit with the rustup URL.
6. Verify by running `agentlens --version`.
7. If the target directory is not on `PATH`, print the `export` line. Do not
   edit any shell rc file.
8. Offer the agent kit.

Flags: `--version`, `--from-source`, `--dir`, `--yes`, `--no-agent-kit`.
Prompts are skipped when stdin is not a TTY, so `curl … | sh` cannot hang.

Being POSIX, this serves WSL and Git Bash on Windows. The MSVC archive is still
published for native Windows users, who download it or use `cargo install`.

### Agent kit

```
agent-kit/
  SKILL.md                  # Claude Code frontmatter and body
  references/
    commands.md             # full flag reference, both binaries
    addressing.md           # address grammar, quoting, escape hatches
    recipes.md              # task -> command patterns
  agent.md                  # agent body, no frontmatter
  frontmatter/
    agent.claude.yaml
    agent.copilot.yaml
  install-kit.sh
```

One agent body; `install-kit.sh` prepends the correct frontmatter per target.
This is what makes "author once" real — there is no second copy to drift.

The two toolchains differ in scope, which drives the installer's shape:

| Toolchain | Skill | Agent | Scope |
|---|---|---|---|
| Claude Code | `~/.claude/skills/agentlens/` | `~/.claude/agents/agentlens-scout.md` | global |
| Copilot | `<repo>/.github/skills/agentlens/` | `<repo>/.github/agents/agentlens-scout.md` | per-repo |

So `install-kit.sh` installs Claude globally and takes a repo path for Copilot,
defaulting to the cwd when it is a git repository.

`SKILL.md` uses progressive disclosure: it stays short and routes to
`references/` on demand, so the common case does not pay for the full flag
reference.

### `agentlens-scout` contract

Input is a list of questions from a primary agent. Output is XML-enveloped
markdown, one `<q>` per question, closed by a `<summary>`.

```
<q id="1" status="answered">
Where is retry backoff computed?

ANSWER: `http/client.py#Client._backoff` — exponential, capped at 30s.

commands:
  agentlens find _backoff --kind function
  agentlens slice http/client.py#Client._backoff

evidence:
  http/client.py L88-L96
    delay = min(2 ** attempt, 30)
</q>

<summary commands_run="9" answered="2" total="3"/>
```

`status` is `answered` or `unanswered`. Caps, expressed as counts the agent can
track without a tokenizer:

- at most 4 commands per question, 10 across the whole report
- at most 25 lines of evidence per question; past that, re-run with
  `--signature-only` or `--budget` rather than pasting more
- exhausting the per-question budget yields `status="unanswered"` recording what
  was tried. The agent never infers an answer it did not observe.

Every question in the input appears in the output, in order, whether or not it
was answered. A missing `<q>` means the response was truncated.

### README

A rewrite, not a replacement. The addressing, commands, and `doclens` sections
are accurate and stay. Changes:

- Badges: CI status, release, license, MSRV
- Install leads with `install.sh`; `cargo install` becomes the fallback
- New agent-kit section
- GitHub alerts: `[!WARNING]` pre-1.0 with no published release and an unstable
  CLI surface; `[!NOTE]` `agentlens` parses Python only while `doclens` handles
  json, yaml and markdown; `[!IMPORTANT]` the `PATH` requirement
- Prose passed through the `humanizer` skill

## Verification

`shellcheck -x` and `shfmt -d` on both shell scripts. Both tools are installed
locally, and `shellcheck -x` is already a pre-commit hook, so no config change is
needed to cover the new files.

`actionlint` on `release.yml`. `scripts/ci.sh` green as a regression check — this
branch changes no Rust.

End-to-end dry runs: `install.sh` against the live API with zero releases,
confirming it takes the source-build path cleanly; `install-kit.sh` into a
scratch directory, with both deployed layouts diffed against the canonical
source.

## Skill test results

The skill was built test-first per `superpowers:writing-skills`. Three haiku
subagents answered the same three questions about a Python fixture, with the
binaries on `PATH`, before and after the skill existed.

Baseline, without the skill: **none of the three ran `agentlens` even once.**
Two used `find` and `grep`; the third reached for an unrelated `pysymbols` tool
in the operator's global install. Tool calls ranged from 7 to 18.

With the skill: **all three used `agentlens` as the primary tool.** One still
fell back to `grep` for a final check.

The more useful signal is convergence on answer quality. Asked whether the
package had dead code, the baseline reps gave three different answers, listing
six, five and one item; two of them reported unused imports and constants, which
`agentlens dead` does not analyse. With the skill's caveat that `dead` covers
unreferenced functions and classes only, all three returned the single genuine
case and correctly identified the flagged public classes as false positives.

### Refactor round: the `map` habit

The first version left a real gap. Agents adopted the tool but used it badly,
spending four to five `map` calls locating a symbol the question had already
named. Two causes, both in the skill rather than the agents:

- The lead table's first row was "What's in this file? → `map`", so `map` read
  as the entry point.
- The references documented each command in isolation. An agent could know every
  flag and still choose a poor sequence, because no page showed a trajectory.

The fix keys the lead table on **what the question already gives you** rather
than on how it is phrased, states that `map` is orientation and not lookup, and
adds worked trajectories to `references/recipes.md` that show the two-command
path alongside the tempting wrong path and its cost. `agent.md` carried the same
table and got the same restructure.

Re-tested with three fresh subagents on an equivalent fixture:

| | First version | After the fix |
|---|---|---|
| `map` calls per rep | 4, 5, 3 | 0, 0, 0 |
| Total commands per rep | 6 to 12 | 4, 4, 5 |
| Opened with `find --kind definition` | 1 of 3 | 3 of 3 |

All three converged on the same sequence: `find` to resolve the name, `packet`
for the body and its neighbours, then `dead` and `callers`. Command count
roughly halved.

Residual, minor: one rep still ran a shell `find . -name "*.py"` before starting,
an orientation habit the skill does not address.

### Second refactor: the redundant follow-up

Comparing the three transcripts against each other showed further waste. `packet`
returns the body *and* the callers with confidence tiers, so a `callers` or
`slice` on the same symbol afterwards re-fetches what the agent already holds.
Two of three reps did exactly that. The proof is internal to the run: the third
rep never called `callers` on the target yet reported all three call sites with
their tiers, which it could only have read out of `packet`.

Cause: the questions were numbered, and agents issued one command per question.
The skill described what `packet` *returns* and left the reader to infer that a
follow-up was pointless.

Fix: an explicit list of what one `packet` call already gives you, a section on
mapping the whole question batch to commands before running any, and a common
mistakes table.

### Honest eval

Earlier rounds told the subagents where the binaries were and to add them to
`PATH`. That tests instruction-following, not the skill. This round gave them
only the path to `SKILL.md`, the codebase, and the questions. The binaries were
reachable because `install.sh` had put them on `PATH`, as it would for any user.

Four fresh reps:

| | Baseline | v1 | v2 | v3, honest prompt |
|---|---|---|---|---|
| Used `agentlens` | 0/3 | 3/3 | 3/3 | 3/4 |
| `map` calls per rep | n/a | 4, 5, 3 | 0, 0, 0 | 0, 0, 0 |
| Redundant call after `packet` | n/a | 2/3 | 2/3 | 0/4 |
| Reached the 3-command optimum | 0 | 0 | 0 | 3/4 |
| Binaries named in the prompt | yes | yes | yes | no |

Three of four ran exactly `find --kind definition`, `packet`, `dead` — the
minimum, with `packet` answering two questions at once.

The fourth fell back to grep and three whole-file reads. Its transcript shows it
read the skill and planned the correct path first: the abandonment came after
`python -m agentlens` and `python3 -m agentlens` both returned "No module named
agentlens", which it took as proof the tool was absent. It never tried the bare
command. Removing the binary hint moved adoption from 3/3 to 3/4, which means the
earlier rounds were flattering the skill rather than measuring it.

## Closing the two abandonment paths

Both remaining defects were documentation gaps, not agents ignoring guidance.
Each was diagnosed from the commands a rep actually ran.

**`python -m agentlens`.** Nothing in the skill said these are standalone
executables, so an agent that assumed a Python package got a message that reads
like absence. Fixed with a leading "How to invoke it" section naming the three
invocations that always fail and `which agentlens` as the existence check. The
`curl ... | sh` install line was removed at the same time: telling an agent to
pipe a remote script into a shell is not an acceptable instruction to leave in a
document it will follow unprompted.

**`--root` does not scope `find`.** A rep tried to point `find` at the fixture
with `--root` rather than `cd`. Verified directly: from an unrelated directory,
`agentlens find backoff --kind definition --root <fixture>` prints `no match` and
exits **0** — indistinguishable from the symbol not existing. The rep ran
`--help`, then spent three `map` calls orienting. `--root` only tells the
index-backed commands where the repo root is. Fixed by stating that paths resolve
against cwd, and that a bare `no match` on a symbol you were told about means
check the directory first.

Six reps per round, same fixture, same prompt giving only the skill path, the
project path, and the three questions:

| | v3 | v4, invocation fix | v5, scoping fix |
|---|---|---|---|
| Used `agentlens` | 3/4 | 6/6 | 6/6 |
| Checked with `which` first | n/a | 6/6 | 6/6 |
| Total `map` calls | 0 | 4 | **0** |
| Reps deviating | 1/4 | 0/6 | 0/6 |
| Answered correctly | 4/4 | 6/6 | 6/6 |

Every rep in the last round also declined to report `dead`'s class-level hits as
confirmed, which is the caveat the skill spends a paragraph on.

Residual waste: most reps spend one call on `find retry` before `find backoff`.
The question says "retry backoff", so this is a reasonable first guess rather
than a defect, and it costs one command.

## Known limitation

`release.yml` cannot be fully proven without pushing a tag. `workflow_dispatch`
into a draft release is as far as this branch goes. The download path in
`install.sh` is therefore exercised only against a draft or a synthetic fixture,
not a real published release. This is stated rather than papered over.
