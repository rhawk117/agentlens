# Command reference

Complete flag surface for both binaries, as of v0.1.0. Generated from `--help`.

## Global flags

Accepted by every `agentlens` subcommand:

| Flag | Default | Meaning |
|---|---|---|
| `--budget <n>` | `4000` | Token ceiling. Output degrades rather than truncating. |
| `--json` | off | Machine-readable output. |
| `--no-color` | off | Never emit ANSI. Colour is off by default already. |
| `--quiet` | off | Suppress the trailing summary line. |
| `--no-cache` | off | Ignore and do not write `.agentlens-cache`. |
| `--root <path>` | `.` | Repo root for index-backed commands. |

`doclens` accepts `--budget`, `--json`, `--no-color` and `--quiet` only. It is
stateless, so it has no `--root` and no cache.

## `agentlens`

There is no `index` subcommand. Indexing happens implicitly for the commands
that need it (`callers`, `packet`, `dead`) and is cached under
`.agentlens-cache`.

### `slice <address>`

Extract a definition.

| Flag | Meaning |
|---|---|
| `--signature-only` | Signature lines only, no body. |
| `--no-decorators` | Exclude decorators from the span. |

The span starts at the first decorator and excludes the leading comment block.
`@overload` stubs and conditional definitions return **all** matching spans in
source order, each with its own header.

### `map [path]`

Outline a file, or orient in a directory. Path defaults to `.`.

| Flag | Default | Meaning |
|---|---|---|
| `--depth <n>` | `2` | Nesting depth for a file; directory depth for a repo map. |
| `--kind <k>` | `all` | `function`, `class`, `variable`, `all`. |

Given a file, returns a symbol outline with signatures and no bodies. Given a
directory, returns repo orientation instead.

### `find <pattern> [paths...]`

Kind-aware search. The pattern is a regex unless `--exact` is passed.

| Flag | Default | Meaning |
|---|---|---|
| `--kind <k>` | `any` | Restrict to `definition`, `call`, `reference`. |
| `--exact` | off | Whole-symbol match rather than regex. |
| `--include-comments` | off | Search comment bodies too. |
| `--include-strings` | off | Search string bodies too. |
| `--context <c>` | `symbol` | How much surrounding context to show. |

### `literals [paths...]`

Extract string, number and regex literals.

| Flag | Default | Meaning |
|---|---|---|
| `--kind <k>` | `all` | Literal kind to extract. |
| `--match <pat>` | none | Only literals matching this pattern. |
| `--min-len <n>` | `2` | Skip literals shorter than this. |
| `--in <address>` | none | Restrict to one symbol's body. |
| `--no-group` | off | List every occurrence instead of grouping by value. |
| `--no-skeleton` | off | Keep interpolation verbatim instead of normalising to `<>`. |
| `--include-docstrings` | off | Include docstrings. |

### `callers <address>`

Direct callers of a symbol, with confidence tiers.

| Flag | Meaning |
|---|---|
| `--no-tests` | Skip call sites in test files. |

Confidence tiers matter: a call resolved through an unambiguous import is
reported with higher confidence than a bare attribute access that merely shares
a name. Do not report a low-confidence hit as a definite call site.

### `packet <address>`

Context packet: the symbol's body, plus caller and callee signatures and types.
One call where you would otherwise run `slice`, `callers`, and several more
`slice` calls on the callees.

| Flag | Meaning |
|---|---|
| `--no-tests` | Skip call sites in test files. |

### `dead`

Dead-code candidates across the repo. Takes no path argument; use `--root`.

**Candidates, not conclusions.** Dynamic dispatch, reflection, framework entry
points, and anything called only from outside the indexed tree will appear here
and may be perfectly live. Verify before recommending a deletion.

## `doclens`

Stateless and file-addressed: no index, no cache. Slices are byte spans rather
than re-serialised values, so comments, key order and formatting survive.

### `slice <address>`

| Flag | Meaning |
|---|---|
| `--with-key` | Include the key or heading line in the span. |

### `map [path]`

| Flag | Default | Meaning |
|---|---|---|
| `--depth <n>` | `2` | Nesting depth. |

Given a directory, lists documents instead of outlining one.

### `find <pattern> [paths...]`

| Flag | Meaning |
|---|---|
| `--exact` | Whole-value match rather than regex. |
| `--keys` | Match keys only. |
| `--values` | Match values only. |

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Found what was asked for. |
| `1` | No match. For `map` and `slice`, the error lists what *is* there. |
| `2` | Usage error — bad flag, malformed address. |

A `1` is information, not a failure. When `slice` misses, its output names the
symbols that actually exist, which is usually enough to correct the address
without a second exploratory command.
