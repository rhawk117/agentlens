# agentlens

Symbol-addressed code intelligence for coding agents. The currency is **tokens
per useful fact**, not milliseconds.

An agent reading a 900-line file to see one 30-line method spends ~9,000 tokens.
The same fact through `agentlens slice` costs ~150.

## Status

| Phase | Scope | State |
|---|---|---|
| MVP-0 | `slice` and `map` on a single Python file | shipped |
| Phase 1 | `map` on a directory, `find`, `literals` | shipped |
| Phase 2 | index, callers, context packet, dead-code candidates | planned |

## Install

```
cargo install --path crates/agentlens-cli
```

## Addresses

Everything is addressed symbolically and re-resolved on every call. Line numbers
are supplementary output, never the address.

```
file.py#Symbol
file.py#Class.method
file.py#Class.Inner.method
file.py#                 outline of the file
file.py#L10-L20          literal line span, escape hatch
```

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
| `--budget <n>` | 4000 | Token ceiling; degrades rather than truncating |
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

## Global flags

| Flag | Meaning |
|---|---|
| `--budget <n>` | Token ceiling, all commands |
| `--json` | Machine-readable output |
| `--no-color` | Accepted; agentlens never emits ANSI |
| `--quiet` | Suppress the summary line |

Exit codes: **0** found, **1** not found, **2** error. An agent can branch
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

Snapshots live in `crates/agentlens-cli/tests/snapshots`. Regenerate with
`AGENTLENS_UPDATE_SNAPSHOTS=1 cargo test --workspace`.

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
