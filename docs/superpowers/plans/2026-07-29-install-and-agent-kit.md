# Install Script and Agent Kit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a release pipeline, a platform-detecting install script, a
skill/subagent kit for both Claude Code and Copilot, and a rewritten README — as
one PR from `chore/plan-alpha` into `dev`.

**Architecture:** `release.yml` publishes archives on a four-target native
matrix under a fixed asset contract. `install.sh` consumes that contract,
falling back to a source build when no asset matches. `agent-kit/` holds one
authored copy of the skill and agent; `install-kit.sh` deploys it to both
toolchains. The README documents all three.

**Tech Stack:** GitHub Actions, POSIX `sh`, Rust 1.97.1 workspace
(`agentlens-cli`, `doclens-cli`).

Full rationale, rejected alternatives, and the tokenizer measurements behind the
agent's output format are in
`docs/superpowers/specs/2026-07-29-install-and-agent-kit-design.md`.

## Global Constraints

- Repo is `rhawk117/agentlens`. Binaries are `agentlens` and `doclens`.
- MSRV 1.97, edition 2024, pinned by `rust-toolchain.toml` to `1.97.1`.
- Default install dir `~/.local/bin`. Never edit a user's shell rc.
- `install.sh` and `install-kit.sh` are POSIX `sh`, not bash. `shellcheck -x`
  is a pre-commit hook and will run on them.
- No releases exist. A 404 from `releases/latest` is an expected outcome, not
  an error path.
- Asset contract, referenced by both `release.yml` and `install.sh`:
  `agentlens-<tag>-<target>.tar.gz` (`.zip` for windows-msvc), plus `SHA256SUMS`.
  Each archive holds both binaries, `LICENSE`, and `README.md`.
- Targets: `x86_64-unknown-linux-gnu`, `x86_64-apple-darwin`,
  `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`.
- This branch changes no Rust source. `scripts/ci.sh` must stay green.
- `scripts/**` is not modified. New scripts live at the repo root and in
  `agent-kit/`.

---

### Task A: Release pipeline

**Files:**
- Create: `.github/workflows/release.yml`

**Interfaces:**
- Produces: the asset contract above. Task B depends on the exact archive and
  `SHA256SUMS` names.

- [ ] **Step 1: Write the workflow**

Trigger on `push: tags: ['v*']` and `workflow_dispatch` (draft release, so the
pipeline can be exercised without consuming a tag). Matrix:

| `os` | `target` | `archive` |
|---|---|---|
| `ubuntu-latest` | `x86_64-unknown-linux-gnu` | `tar.gz` |
| `macos-13` | `x86_64-apple-darwin` | `tar.gz` |
| `macos-latest` | `aarch64-apple-darwin` | `tar.gz` |
| `windows-latest` | `x86_64-pc-windows-msvc` | `zip` |

Build with `cargo build --release --locked --target <target> -p agentlens-cli -p doclens-cli`.
Stage both binaries plus `LICENSE` and `README.md`, archive, upload as an
artifact named for the target.

A final `release` job (`needs: build`) downloads all artifacts, runs
`sha256sum * > SHA256SUMS`, and publishes with `gh release create`. Draft when
triggered by `workflow_dispatch`.

Permissions: `contents: write` on the release job only.

- [ ] **Step 2: Lint it**

Run: `actionlint .github/workflows/release.yml`
Expected: no output, exit 0. If `actionlint` is unavailable, run
`uv run --with yamllint yamllint -d relaxed .github/workflows/release.yml` and
note the weaker check.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow for four native targets"
```

---

### Task B: Install script

**Files:**
- Create: `install.sh`

**Interfaces:**
- Consumes: Task A's asset contract.
- Produces: `install.sh --agent-kit` invokes `agent-kit/install-kit.sh` from
  Task C. Write the call site in this task; it is exercised in Task C.

- [ ] **Step 1: Write the script**

POSIX `sh`, `set -eu`. Structure:

- `detect_target()` — `uname -s`/`uname -m` to a triple. `Linux`+`x86_64` →
  `x86_64-unknown-linux-gnu`; `Darwin`+`arm64` → `aarch64-apple-darwin`;
  `Darwin`+`x86_64` → `x86_64-apple-darwin`. Anything else returns empty,
  which routes to the source build rather than failing.
- `resolve_version()` — `--version` if given, else parse `tag_name` from
  `https://api.github.com/repos/rhawk117/agentlens/releases/latest`. No `jq`
  dependency; use `grep`/`sed`. A 404 or empty result returns empty.
- `install_from_release()` — download archive and `SHA256SUMS` to a temp dir
  cleaned by an `EXIT` trap, verify (`sha256sum -c` or `shasum -a 256 -c`),
  extract, `install -m 755` both binaries into the target dir.
- `install_from_source()` — require `cargo`; use `--path` when run inside the
  repo, `--git https://github.com/rhawk117/agentlens` otherwise.
- `check_path()` — if the target dir is absent from `PATH`, print the exact
  `export PATH="$dir:$PATH"` line. Never write to a shell rc.
- `offer_agent_kit()` — skip when `--no-agent-kit`, when `--yes` is absent and
  stdin is not a TTY, or when `agent-kit/install-kit.sh` is missing.

Flags: `--version <tag>`, `--from-source`, `--dir <path>`, `--yes`,
`--no-agent-kit`, `--help`.

- [ ] **Step 2: Lint and format**

Run: `shellcheck -x install.sh && shfmt -d -i 4 install.sh`
Expected: both silent, exit 0.

- [ ] **Step 3: Verify the no-release path**

Run: `sh install.sh --help` then `sh install.sh --dir /tmp/al-test --yes`
Expected: reports that no published release was found, falls through to the
source build, installs both binaries, and `\/tmp/al-test/agentlens --version`
runs. This is the live path today and is the primary verification for this task.

- [ ] **Step 4: Verify argument handling**

Run: `sh install.sh --dir /tmp/al-test2 --from-source --yes --no-agent-kit`
Expected: succeeds without prompting and without touching `~/.claude`.

- [ ] **Step 5: Commit**

```bash
git add install.sh
git commit -m "feat: add install script with release and source paths"
```

---

### Task C: Agent kit

**Files:**
- Create: `agent-kit/SKILL.md`
- Create: `agent-kit/references/commands.md`
- Create: `agent-kit/references/addressing.md`
- Create: `agent-kit/references/recipes.md`
- Create: `agent-kit/agent.md`
- Create: `agent-kit/frontmatter/agent.claude.yaml`
- Create: `agent-kit/frontmatter/agent.copilot.yaml`
- Create: `agent-kit/install-kit.sh`

**Interfaces:**
- Consumes: invoked by `install.sh` from Task B.
- Produces: `agentlens-scout` agent and `agentlens` skill in both layouts.

- [ ] **Step 1: Write `SKILL.md` using the writing-skills skill**

Invoke `superpowers:writing-skills`. Frontmatter carries `name: agentlens` and a
`description` starting "Use when…". The body stays short and routes to
`references/` — progressive disclosure, so the common case does not load the
full flag reference. Cover: when to reach for the tool, the address grammar in
brief, the command table, and dispatching `agentlens-scout` with a question list.

- [ ] **Step 2: Write the three reference files**

`commands.md` — every flag for both binaries, from the README's tables and
`--help`. `addressing.md` — address grammar, quoting rules, `L10-L20` escape
hatch, `doclens` `.`/`[n]`/`/` forms. `recipes.md` — task-to-command patterns.

- [ ] **Step 3: Write `agent.md` (body only, no frontmatter)**

Input is a list of questions. Output is the XML-enveloped markdown from the
spec. Caps: 4 commands per question, 10 total, 25 evidence lines per question.
Unanswered questions are emitted with `status="unanswered"` and what was tried;
the agent never infers an unobserved answer. Every input question appears in the
output in order.

- [ ] **Step 4: Write the two frontmatter files**

`agent.claude.yaml`:
```yaml
name: agentlens-scout
description: Answers a list of questions about a codebase using agentlens and doclens, returning capped XML-enveloped findings with the exact commands run. Use when a primary agent needs several code questions answered without spending its own context reading files.
model: claude-haiku-4.5
tools: Bash, Read
```

`agent.copilot.yaml` — identical minus the `model` line.

- [ ] **Step 5: Write `install-kit.sh`**

POSIX `sh`. Assembles `---` + frontmatter + `---` + `agent.md` per target.

- `--claude` (default on): skill to `~/.claude/skills/agentlens/`, agent to
  `~/.claude/agents/agentlens-scout.md`
- `--copilot [dir]`: skill to `<dir>/.github/skills/agentlens/`, agent to
  `<dir>/.github/agents/agentlens-scout.md`; defaults to cwd when it is a git
  repo, otherwise requires an explicit path
- `--dry-run` prints the destinations without writing

- [ ] **Step 6: Lint**

Run: `shellcheck -x agent-kit/install-kit.sh && shfmt -d -i 4 agent-kit/install-kit.sh`
Expected: silent, exit 0.

- [ ] **Step 7: Verify deployment into a scratch tree**

```bash
HOME=/tmp/al-kit sh agent-kit/install-kit.sh --claude
HOME=/tmp/al-kit sh agent-kit/install-kit.sh --copilot /tmp/al-kit/repo
```
Expected: `/tmp/al-kit/.claude/agents/agentlens-scout.md` starts with `---`,
contains `model: claude-haiku-4.5`, and its body matches `agent-kit/agent.md`.
The Copilot copy contains no `model:` line. Verify the bodies are identical:

```bash
diff <(sed '1,/^---$/d;1,/^---$/d' /tmp/al-kit/.claude/agents/agentlens-scout.md) agent-kit/agent.md
```
Expected: no differences.

- [ ] **Step 8: Commit**

```bash
git add agent-kit
git commit -m "feat: add agentlens skill and scout agent for claude code and copilot"
```

---

### Task D: README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite**

Preserve the addressing, commands, and `doclens` sections — they are accurate.
Add badges (CI, release, license, MSRV), lead Install with `install.sh`, demote
`cargo install` to a fallback, add an agent-kit section, and add three GitHub
alerts: `[!WARNING]` pre-1.0 with no published release; `[!NOTE]` `agentlens`
handles Python only while `doclens` covers json/yaml/markdown; `[!IMPORTANT]`
the `PATH` requirement.

- [ ] **Step 2: Run prose through the humanizer skill**

Invoke `humanizer` on the new and rewritten prose. It does not apply to code
blocks, command output, or tables.

- [ ] **Step 3: Verify links and badges**

Run: `grep -o 'https://[^)"]*' README.md | sort -u`
Check each badge URL resolves against `rhawk117/agentlens` and the workflow
name `ci`.

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: rewrite readme with badges, alerts, and install guidance"
```

---

### Task E: Full verification and PR

- [ ] **Step 1: Gate**

Run: `bash scripts/ci.sh`
Expected: all stages pass, 81 tests. No Rust changed, so a failure here is a
regression from something else.

- [ ] **Step 2: Shell and workflow lint over everything new**

Run: `shellcheck -x install.sh agent-kit/install-kit.sh && shfmt -d -i 4 install.sh agent-kit/install-kit.sh && actionlint`
Expected: silent, exit 0.

- [ ] **Step 3: Clean the scratch dirs**

```bash
rm -rf /tmp/al-test /tmp/al-test2 /tmp/al-kit
```

- [ ] **Step 4: Push and open the PR**

```bash
git push -u origin chore/plan-alpha
gh pr create --base dev --title "chore: install script, agent kit, and readme" --body-file <file>
```

Use `--body-file`. `cat` is aliased to `bat` on this machine, so heredocs into
`gh` are unreliable.

- [ ] **Step 5: Watch CI**

Dispatch the `ci-watcher` subagent with the PR number. On FAIL, fix in source
and dispatch a new watcher. Do not merge without an observed PASS.

## Self-review notes

- The spec's every section maps to a task: release pipeline → A, install script
  → B, agent kit and scout contract → C, README → D, verification → E.
- The asset contract appears verbatim in Global Constraints so Tasks A and B
  cannot drift apart.
- `install.sh`'s call into `agent-kit/install-kit.sh` is written in Task B but
  only exercised in Task C. If Tasks B and C are run by different agents, B's
  verification must not assert the kit installs.
- Not investigated: whether GitHub Copilot actually reads `.github/skills/`.
  The layout follows Copilot's documented custom-agent convention, but this was
  not empirically confirmed. The Claude Code path is the one verified here.
