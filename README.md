# agentlens

Symbol-addressed code intelligence for coding agents. The currency is **tokens
per useful fact**, not milliseconds.

An agent reading a 900-line file to see one 30-line method spends ~9,000 tokens.
The same fact through `agentlens slice` costs ~150.

## Status

| Phase | Scope | State |
|---|---|---|
| MVP-0 | `slice` and `map` on a single Python file | shipped |
| Phase 1 | `map` on a directory, `find`, `literals` | planned |
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
| `--depth <n>` | 2 | Nesting depth |
| `--kind <k>` | all | `function`, `class`, `variable`, `all` |

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
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Snapshots live in `crates/agentlens-cli/tests/snapshots`. Regenerate with
`AGENTLENS_UPDATE_SNAPSHOTS=1 cargo test --workspace`.

## Non-goals

Type inference, editing code, LSP or daemon mode, a query language of our own,
beating ripgrep at raw text search.
