# agentlens

[![ci](https://github.com/rhawk117/agentlens/actions/workflows/ci.yml/badge.svg)](https://github.com/rhawk117/agentlens/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/rhawk117/agentlens?sort=semver&display_name=tag)](https://github.com/rhawk117/agentlens/releases)
[![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](LICENSE)
[![rust](https://img.shields.io/badge/rust-1.97%2B-orange)](rust-toolchain.toml)

Symbol-addressed code intelligence for coding agents. The currency is **tokens
per useful fact**, not milliseconds.

An agent reading a 900-line file to see one 30-line method spends ~9,000 tokens.
The same fact through `agentlens slice` costs ~150.

> [!WARNING]
> Pre-1.0. The CLI surface may change without a deprecation period until 1.0.

> [!NOTE]
> `agentlens` resolves symbols in **Python only**. `doclens` handles json, yaml
> and markdown in any repository. For other languages, ripgrep is still the
> right tool — see [Non-goals](#non-goals).

## Status

| Phase | Scope | State |
|---|---|---|
| MVP-0 | `slice` and `map` on a single Python file | shipped |
| Phase 1 | `map` on a directory, `find`, `literals` | shipped |
| Phase 2 | index, callers, context packet, dead-code candidates | shipped |
| Doc tool | `doclens` for json, yaml and markdown | shipped |

## Install

```
curl -fsSL https://raw.githubusercontent.com/rhawk117/agentlens/dev/install.sh | sh
```

The script detects your platform and installs both binaries into `~/.local/bin`.
Where a release binary exists it downloads and checksums it; otherwise it builds
from source, which needs Rust 1.97 or newer.

| Flag | Meaning |
|---|---|
| `--version <tag>` | Install a specific release rather than the latest |
| `--dir <path>` | Install somewhere other than `~/.local/bin` |
| `--from-source` | Skip the download and build with cargo |
| `--yes` | Answer every prompt yes; implied when stdin is not a terminal |
| `--no-agent-kit` | Skip the skill and agent prompt |

> [!IMPORTANT]
> `~/.local/bin` must be on your `PATH`. The installer checks and prints the
> `export` line if it is missing, but it will not edit your shell profile for
> you.

Prefer cargo, or already have the repo cloned:

```
cargo install --path crates/agentlens-cli
cargo install --path crates/doclens-cli
```

Windows users: `install.sh` covers WSL and Git Bash. For native Windows, take
the `x86_64-pc-windows-msvc` archive from the releases page or use `cargo
install`.

## Agent kit

A skill and a subagent that teach a coding agent to drive these binaries,
for both Claude Code and GitHub Copilot.

```
sh agent-kit/install-kit.sh              # Claude Code, into ~/.claude
sh agent-kit/install-kit.sh --copilot    # Copilot, into ./.github
sh agent-kit/install-kit.sh --dry-run    # show the destinations, write nothing
```

The skill maps question shapes onto commands, so an agent reaches for `packet`
instead of reading a file. The `agentlens-scout` subagent takes a list of
questions and returns capped findings with the exact commands it ran — the
answers land in the caller's context without the file contents behind them.

Both are authored once under `agent-kit/`; the script assembles the frontmatter
each toolchain expects. Claude Code agents install globally, Copilot agents
per-repository, which is why the Copilot flag takes a directory.

## Addresses

Everything is addressed symbolically and re-resolved on every call. Line numbers
are supplementary output, never the address.

```
file.py#Symbol
file.py#Class.method
file.py#Class.Inner.method
file.py#__module__       imports and module-level code
file.py#                 outline of the file
file.py#L10-L20          literal line span, escape hatch
```

Module-level constants are symbols too, so `settings.py#MIDDLEWARE` addresses a
value as readily as a function.

Near-miss forms are accepted and rewritten, with a `note:` on stderr naming what
was read — `file.py` alone, `file.py 10 20`, `file.py:10-20`, `file.py::Symbol`.
Rewriting happens only when the file exists, so nothing is guessed. These are a
courtesy, not the idiom; write the canonical form.

## Commands

### `slice` — extract a definition

```
agentlens slice src/api/users.py#UserService.create_user
```

```
src/api/users.py#UserService.create_user  L21-L28  8 lines
    @transaction.atomic
    @audit("user.create")
    def create_user(self, email: str, name: str) -> User:
        ...

1 match, 8 lines
```

The span starts at the first decorator and excludes the leading comment block.
`@overload` stubs and conditional definitions return **all** matching spans in
source order, each with its own header.

| Flag | Default | Meaning |
|---|---|---|
| `--signature-only` | off | Signature lines only, no body |
| `--no-decorators` | off | Exclude decorators from the span |
| `--budget <n>` | 4000 | Token target; degrades rather than truncating |
| `--json` | off | Machine-readable |

### `map` — what's in here

```
agentlens map src/api/users.py
```

A file gets a symbol outline with signatures and no bodies. A missing symbol
exits 1 and lists what is actually there.

| Flag | Default | Meaning |
|---|---|---|
| `--depth <n>` | 2 | Nesting depth for a file outline; directory depth for a repo map |
| `--kind <k>` | all | `function`, `class`, `variable`, `all` |

Given a directory, `map` gives repo orientation instead: languages with file and
line counts, the directory tree to depth 2, detected entry points (`__main__`
guards, `main` functions, route decorators, console-script entries) and the
manifest files found.

```
.  5 source files  121 lines

languages
  python    5 files  121 lines

tree (depth 2)
  src/  4 files  109 lines
    api/  2 files  82 lines

entry points
  src/api/routes.py#health  @app.get("/health")
  src/cli.py#L12            __main__ guard
  src/cli.py#main           console script `fixture`
```

### `find` — kind-aware search

```
agentlens find create_user
```

```
src/api/routes.py
  L14     call        create_user  in #create

src/api/users.py
  L23     definition  create_user  in #UserService.create_user

2 matches in 2 files
```

Comments and string bodies are excluded by default — that is most of the value
over grep. Structural context beats line context, so there is no `-A`/`-B`:
`--context symbol` names the enclosing symbol, `--context none` gives the
address alone.

| Flag | Default | Meaning |
|---|---|---|
| `--kind <k>` | any | `definition`, `call`, `reference`, `any` |
| `--exact` | off | Whole-symbol match rather than regex |
| `--include-comments` | off | Search comment bodies too |
| `--include-strings` | off | Search string bodies too |
| `--context <c>` | symbol | `symbol` or `none` |
| `--expand` | off | List references and test hits instead of counting them |

Definitions rank first in their own block; references and test hits collapse to
counts unless `--expand` is passed. `find` scans occurrences, so it does not
match module-level constants — a miss on a name that is a real symbol says so
and points at `sym`.

### `sym` — one symbol by name

```
agentlens sym MAX_RETRIES
```

```
src/api/users.py#MAX_RETRIES  MAX_RETRIES = 3
```

Address, kind, and for a constant its value. Resolves against the symbol index,
so unlike `find` it matches module-level constants. Accepts `file.py#Symbol` as
well as a bare name. A name with several definitions lists them all and exits
**0** — that is the answer, not a failure — degrading through the usual ladder
so the true total is always stated. Exit **1** means no such symbol.

### `literals` — extract constants

```
agentlens literals --kind string
```

```
"connection refused: <>"  string  x2
  src/core/config.py#describe  L10
  src/core/config.py#banner    L14
```

Interpolation is normalised to `<>` so a log line can be matched back to the
source that emitted it. Docstrings are excluded by default.

| Flag | Default | Meaning |
|---|---|---|
| `--kind <k>` | all | `string`, `number`, `regex`, `all` |
| `--match <pat>` | — | Filter by value |
| `--min-len <n>` | 2 | Skip trivial strings |
| `--in <address>` | — | Scope to one symbol |
| `--no-group` | grouped | List every occurrence instead of grouping by value |
| `--no-skeleton` | skeleton on | Keep interpolation verbatim |
| `--include-docstrings` | off | Include docstrings |

### `callers` — direct callers, depth 1

```
agentlens callers src/api/users.py#UserService.create_user
```

```
src/api/users.py#UserService.create_user  def create_user(self, email: str, name: str) -> User:

certain
  src/cli.py#main                       L8   2 args         imported by name

likely
  src/api/routes.py#create              L14  2 args         attribute call on `service`, name unique in repo
  tests/test_users.py#test_create_user  L6   2 args [test]  attribute call on `service`

3 callers (2 non-test, 1 test)
```

Results carry a confidence tier — `certain`, `likely`, `possible` — and the
reason for it. Python is the hardest language for this and it is deliberately
first: expect `possible`-tier noise from duck typing and decorators. Test call
sites are tagged `[test]`; `--no-tests` drops them.

### `packet` — context packet

```
agentlens packet src/api/users.py#UserService.create_user
```

The full body of the target, signature-only lines for direct callers and
callees, and the types named in the signature. When the budget bites, the body
survives and the signature sections go first.

### `dead` — dead-code candidates

```
agentlens dead
```

Zero call sites, minus exported (`__all__`, package `__init__.py`), minus entry
points, minus tests, minus registration decorators. Labelled **candidates**,
never "dead" — reflection and DI defeat this.

## The index

Index-backed commands (`callers`, `packet`, `dead`) keep an in-repo
`.agentlens-cache/` containing a `.gitignore` of `*`, so it conceals itself.
Freshness is a stat walk over `(mtime_ns, size)`, then a blake3 content hash
over only the candidates — git rewrites mtimes wholesale on checkout and rebase,
so mtime alone over-invalidates badly.

The cache is a cache, never a source of truth: missing or corrupt means a silent
rebuild, and there is no `index` command anyone has to remember. `--no-cache`
skips reading and writing it; `--root <path>` sets the repo root.

## Global flags

| Flag | Meaning |
|---|---|
| `--budget <n>` | Token target, all commands. Estimated, so ~7% of calls run over |
| `--json` | Machine-readable output |
| `--no-color` | Accepted; agentlens never emits ANSI |
| `--quiet` | Suppress the summary line |
| `--no-cache` | Ignore and do not write `.agentlens-cache` |
| `--root <path>` | Repo root for index-backed commands |

Exit codes: **0** found, **1** not found or not applicable, **2** genuine tool
fault. A mistyped address and an unsupported format are both **1**; **2** is
reserved for faults a caller cannot fix by rephrasing. An agent can branch
without parsing output.

## Output contracts

- Answer with the fact, not the evidence.
- Intern paths: emit a path once, results beneath it.
- Never truncate silently — always `showing 50 of 400` with how to get more.
- Every output is a valid input: results are addresses you can feed to `slice`.
- Advertise the next call.
- Degradation ladder when a budget is hit: full → summary → counts.
- Deterministic: stable sort, no timestamps, no elapsed times, no ANSI.

## Verification gate

```
scripts/ci.sh
```

This is the same entry point GitHub Actions runs. Individual stages:
`scripts/format.sh`, `scripts/lint.sh`, `scripts/build.sh`, `scripts/test.sh`,
`scripts/audit.sh`, `scripts/coverage.sh`.

Snapshots live in `crates/agentlens-cli/tests/snapshots` and
`crates/doclens-cli/tests/snapshots`. Regenerate with
`AGENTLENS_UPDATE_SNAPSHOTS=1 cargo test --workspace`.

## `doclens` — the doc tool

A second binary sharing the same core crate. Stateless and file-addressed: no
index, no cache. Slices are **byte spans, not re-serialised values**, so
comments, key order and formatting survive intact.

```
doclens slice compose.yaml#services.web
doclens slice compose.yaml#services.web.ports[1]
doclens slice package.json#scripts
doclens slice README.md#Install/From source
doclens map compose.yaml --depth 3
doclens find postgres --values
```

Addresses use `.` and `[n]` for JSON and YAML, and `/` between headings for
Markdown. A quoted step survives dots: `config.json#["a.b"].c`.

```
compose.yaml  yaml  16 entries
  version            L1            "3.9"
  services           L4-L15        {2 keys}
    web              L5-L11        {3 keys}
      image          L5            nginx:1.27
      ports          L7-L8         [2 items]  +2 nested
```

| Command | Flags |
|---|---|
| `slice <address>` | `--with-key` includes the key or heading line in the span |
| `map <path>` | `--depth <n>`; a directory lists documents instead |
| `find <pattern> [paths...]` | `--exact`, `--keys`, `--values` |

Global flags, output contracts and exit codes match `agentlens`.

## Contributing

Bootstrap your environment (installs and verifies the toolchain):

```
./precheck.sh --install
pre-commit install
```

Then run the gate:

```
scripts/ci.sh
```

## Non-goals

Type inference, editing code, LSP or daemon mode, a query language of our own,
beating ripgrep at raw text search.
