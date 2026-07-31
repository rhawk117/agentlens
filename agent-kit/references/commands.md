# Command reference

Flag surface for both binaries, as of v0.2.0, checked against `--help`.

If a flag here disagrees with the binary, the binary is right: run
`agentlens <command> --help`, which is always current and costs less than a
wrong call.

## Global flags

Accepted by every `agentlens` subcommand:

| Flag | Default | Meaning |
|---|---|---|
| `--budget <n>` | `4000` | Token target. Output degrades rather than truncating. Not a hard cap — see below. |
| `--json` | off | Machine-readable output. |
| `--no-color` | off | Never emit ANSI. Colour is off by default already. |
| `--quiet` | off | Suppress the trailing summary line. |
| `--no-cache` | off | Ignore and do not write `.agentlens-cache`. |
| `--root <path>` | `.` | Repo root for index-backed commands. |

`doclens` accepts `--budget`, `--json`, `--no-color` and `--quiet` only. It is
stateless, so it has no `--root` and no cache.

### What `--budget` promises

A **target, not a hard cap.** The tool estimates its own output rather than
running it through a tokenizer, so a call can exceed the number. On measured
output roughly 7% of calls run over, the worst by 28%. Budget for that, and do
not use `--budget` as a hard ceiling on a context window.

The `--json` envelope reports what happened: `tokens` is the estimate, `budget`
is what you asked for, `detail` is the rung that was rendered (`full`,
`summary`, `counts`), and `degraded` is `true` when the output is not `full`.
Read `degraded` rather than guessing from output size.

`--root` affects the index-backed commands only: `sym`, `callers`, `packet`,
`dead`. It does **not** redirect `find`, `map` or `slice`, which resolve paths
against the current directory.

## `agentlens`

There is no `index` subcommand. Indexing happens implicitly for the commands
that need it (`sym`, `callers`, `packet`, `dead`) and is cached under
`.agentlens-cache`.

### `sym <name>`

One symbol by name: address, kind, and for a constant its value. The cheapest
way to turn a name you already have into an address.

A long value is **previewed, not printed in full** — `sym` is a lookup, not a
retrieval. The preview ends with the `slice` address that yields the whole
thing, so fetching a large literal is two calls by design:

```
agentlens sym MIDDLEWARE
# -> django/conf/global_settings.py#MIDDLEWARE  MIDDLEWARE = [ "...", ... (slice ... for the whole value)
agentlens slice django/conf/global_settings.py#MIDDLEWARE
```

Short values are complete in one call, so check the output before assuming you
need the second.

Takes no flags of its own beyond the global set. Resolution is against the
symbol index, so it finds module-level constants that `find` cannot match, and
it accepts `file.py#Symbol` as well as a bare name so it is not a dead end when
a name is ambiguous.

A name matching several definitions is **not an error** — `sym` lists them all
and exits `0`, because "here are the three" is the answer to "where is this".
Under budget pressure the list degrades through the usual ladder to a bare
count, and every rung states the true total, so nothing is silently dropped.
Only a name with no definition at all exits `1`.

That differs from bare-name resolution in `slice`, `callers` and `packet`,
which exit `1` on ambiguity — those commands have to act on exactly one symbol,
so several candidates is a blocker rather than a result.

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
| `--expand` | off | Show whole values instead of collapsing large ones. |
| `--match <pat>` | none | Only symbols whose name matches. File targets only. |

Given a file, returns a symbol outline with signatures and no bodies. Given a
directory, returns repo orientation instead.

Large literal values are **collapsed by default** — `LANGUAGES = [... 187
items]` rather than the whole list, with the address that yields the full
value. One settings file cost 3,741 tokens before this was the default. Use
`--expand` when you genuinely need the values, and expect it to be expensive.

`--match` filters a file outline by symbol name and states how many entries it
dropped. On a directory target it is currently ignored rather than rejected, so
filter a file, not a tree.

When the file has a module preamble, `__module__` is listed first — see
`addressing.md`.

### `find <pattern> [paths...]`

Kind-aware search. The pattern is a regex unless `--exact` is passed.

| Flag | Default | Meaning |
|---|---|---|
| `--kind <k>` | `any` | Restrict to `definition`, `call`, `reference`. |
| `--exact` | off | Whole-symbol match rather than regex. |
| `--include-comments` | off | Search comment bodies too. |
| `--include-strings` | off | Search string bodies too. |
| `--context <c>` | `symbol` | How much surrounding context to show. |
| `--expand` | off | List references and test hits instead of counting them. |

Results are ranked by occurrence kind: definitions first in their own block,
then calls. References and test hits **collapse to counts by default**, because
"where is this defined" is the common question and listing 200 reference rows
answers it expensively. Each collapsed block states its true total, and
`--expand` lists them. Asking for `--kind reference` always lists, since
collapsing the kind you filtered for would answer with a count.

`find` scans occurrences, so `--kind definition` does **not** match module-level
constants: an assignment reads as a reference, not a definition. Use `sym` for
constants.

A miss says which of the two things went wrong, and names the call that fixes
it. Either the filter excluded everything —

```
`SecurityMiddleware` has 2 occurrences, none of them a reference: try `--kind any`
```

— or the name is a symbol `find` cannot see as a definition:

```
`MIDDLEWARE` is a symbol but not an occurrence `find` scans: try `agentlens sym MIDDLEWARE`
```

Run the invocation it names. It is built from what is actually in the index, so
it will not fail the way the original call did.

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
| `1` | Not found, or not applicable. For `map` and `slice`, the output lists what *is* there. |
| `2` | Genuine tool fault. |

A `1` is information, not a failure. When `slice` misses, its output names the
symbols that actually exist, which is usually enough to correct the address
without a second exploratory command.

**A malformed address is `1`, not `2`.** So is an unsupported language or
format: `map notes.txt` is "not applicable", not a failure. `2` is reserved for
faults the caller cannot fix by rephrasing — an unreadable file, a parse
failure, a bad `--kind` value, an invalid `--match` regex. Retrying a `2` with
the same arguments will not help; retrying a `1` with a better address might.

## JSON output

Every command emits JSON on **stdout** under `--json`, on success and on
failure, in both binaries. You never need to parse stderr to learn what went
wrong, and the exit code is unchanged by `--json`.

Four fields are on every envelope. Branch on these:

| Field | Meaning |
|---|---|
| `schema_version` | Envelope shape version. Currently `1`. |
| `tool_version` | The binary's version. |
| `command` | Which subcommand produced this. |
| `found` | Whether the thing asked for was found. |

**The rest of the envelope varies by outcome**, so test for a key before
reading it rather than assuming it is there:

| Outcome | Also carries |
|---|---|
| Anything that rendered text | `detail` (`full`/`summary`/`counts`), `degraded`, `budget`, `tokens` |
| A tool fault | `error` (`kind`, `message`, `suggestion` — the last may be `null`) and `exit` |
| `slice` missing a symbol in a file that exists | `available`, listing the symbols that are there |

Two consequences worth planning for. A missing symbol is **not** an `error`
envelope — it is `found: false` plus `available`, because a miss is not a
fault. And `slice`'s symbol-miss carries neither `error` nor `budget`/`tokens`,
so `degraded` is not a reliable universal signal: read it where it exists, and
treat its absence as "nothing was degraded".

The advertise-the-next-call footer is suppressed under `--json`, since the
addresses in the envelope carry the same information.
