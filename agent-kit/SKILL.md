---
name: agentlens
description: Use when answering questions about code or config you have not read yet - locating a definition, finding call sites, extracting one method from a large file, listing literals, checking for dead code, or slicing a key out of json, yaml or markdown. Applies before reaching for grep, find, or reading a file to answer such a question.
---

# agentlens

Symbol-addressed code intelligence. The currency is **tokens per useful fact**.

Reading a 900-line file to see one 30-line method costs ~9,000 tokens. The same
fact through `agentlens slice` costs ~150.

## How to invoke it

`agentlens` and `doclens` are **standalone executables on your `PATH`**. Run them
directly:

```
agentlens find retry --kind definition
```

They analyse Python, but they are not written in Python and are not Python
packages. These all fail even on a correctly installed system:

```
python -m agentlens ...      uv run agentlens ...      npx agentlens ...
```

Before concluding it is missing, check with `which agentlens`. A failure from
`python -m agentlens` tells you nothing about whether the binary exists. If
`which` comes back empty it genuinely is not installed: say so in your answer and
name the fallback you used instead. Quietly substituting grep hands back an
answer that cost ten times what it should have, and the caller cannot tell it
happened.

## Point it at the right directory

Paths are resolved against your current directory. If the project you were asked
about is somewhere else, `cd` there first.

`--root` will not do this for you. It tells the index-backed commands
(`callers`, `packet`, `dead`) where the repo root is; `find`, `map` and `slice`
ignore it. From the wrong directory, `find` prints `no match` and exits **0** —
identical to the symbol genuinely not existing. A `no match` on a symbol you were
told about means check your directory before you conclude anything.

## Start from what you already know

Pick your first command by what the question gives you. This is the whole
technique — getting it wrong is what turns a two-command answer into eight.

| You already know | First command | Then |
|---|---|---|
| A symbol name | `agentlens find <name> --kind definition` | `agentlens packet <address>` |
| A file and a symbol | `agentlens packet <file>#<Sym>` | done |
| A file, not the symbol | `agentlens map <file>` | `packet` the one you want |
| Neither | `agentlens map <dir> --depth 2` | narrow, then as above |

**A named symbol never needs `map`.** If the question says "the backoff
function", you have a name: `find` resolves it to an address in one call.
Opening files to look for it is the habit this tool exists to replace, and
`map`-ing several files to locate one symbol costs more than the grep it
replaced.

`map` is for orientation — when you cannot name what you are looking for. It is
not a lookup step.

## What one `packet` already gives you

`packet` is the command to know. A single call returns all of this, so after
running it you are already holding:

- the full body of the symbol
- every direct caller, with file, line and confidence tier
- the signatures of everything the symbol calls
- the types named in its signature

One `packet` therefore answers both "what does X do" and "who calls X". Once you
have run it on a symbol, report the callers from that output. Running `callers`
or `slice` on the same symbol afterwards re-fetches what you already have.

## Answer the batch, not one question at a time

Questions arrive in groups, and one command usually covers several of them. Map
the whole set to commands before running anything.

Given "where is the backoff computed", "who calls it", and "is there dead code",
the mapping is two commands and not three: `packet` covers the first two, `dead`
covers the third. Taking the questions in order and issuing a command per
question is what produces the redundant third call.

## Everything else

| Question | Command |
|---|---|
| Just the body, nothing else | `agentlens slice <file>#<Sym>` |
| Who calls X? | `agentlens callers <file>#X [--no-tests]` |
| What strings/numbers are here? | `agentlens literals <path> [--in <addr>]` |
| What's unused? | `agentlens dead --root <dir>` |
| A key in json/yaml/md | `doclens slice <file>#<address>` |
| Search config | `doclens find <pat> [--keys\|--values]` |

Do not `grep`, `find`, or read a file to answer any of these. Those cost 5-15
tool calls and a file's worth of context to produce what one command returns.

## Addresses, briefly

```
file.py#Symbol            file.py#Class.method       file.py#L10-L20
config.yaml#services.web  package.json#scripts.build  README.md#Install/Usage
```

Symbolic and re-resolved every call, so an address survives edits above it.
Full grammar, quoting rules, and the markdown heading form: `references/addressing.md`

## Reading the output

A missing symbol exits `1` **and lists what is actually there**. That list is
the correction — use it instead of running an exploratory `map`.

`dead` and `callers` report *candidates with confidence*, not conclusions.
Dynamic dispatch, framework entry points, and reflection all defeat static
analysis. `dead` finds unreferenced functions and classes; it does not track
unused imports or constants, so do not report those as its findings. Verify
before recommending a deletion.

Every command takes `--budget <n>` (default 4000), which degrades output rather
than truncating it, and `--json` for machine-readable results.

Full flag surface: `references/commands.md`
Worked task-to-command patterns: `references/recipes.md`

## Common mistakes

| Mistake | Instead |
|---|---|
| `map`-ing several files to find a symbol you can name | `find <name> --kind definition` |
| `callers X` right after `packet X` | Read the callers out of the packet |
| `slice X` right after `packet X` | The body is already in the packet |
| One command per question | Map the whole batch first; `packet` covers two |
| Reporting `dead` hits as confirmed | They are candidates; verify with `callers` |
| Calling unused imports or constants "dead code" | `dead` only covers functions and classes |
| `grep`-ing for call sites | grep cannot tell a call from an import or a mention |

## Scope

`agentlens` parses **Python only** today. For other languages it will not
resolve symbols, and grep remains the right tool. `doclens` handles json, yaml
and markdown in any repo.

## Delegating a batch of questions

With several questions at once, dispatch the `agentlens-scout` subagent with the
list. It runs the commands and returns capped findings with the exact commands
it ran, so the answers land in your context without the file contents behind
them.
