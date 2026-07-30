# Address grammar

Everything is addressed symbolically and re-resolved on every call. Line numbers
are supplementary output, never the address. This is the whole point: an address
survives edits above it, a line number does not.

## `agentlens` — code

```
file.py#Symbol                  top-level function, class or constant
file.py#Class.method            method on a class
file.py#Class.Inner.method      arbitrary nesting
file.py#__module__              imports and module-level code
file.py#                        outline of the file, no body
file.py#L10-L20                 literal line span
```

Module-level constants are symbols: `settings.py#MIDDLEWARE` slices the value.

### `#__module__` — the region above the first definition

Everything from the top of the file to the line before the first definition:
the module docstring, the imports, and any module-level statements.

```
agentlens slice django/core/handlers/base.py#__module__
```

This is the address to use for "what does this file import" and "what runs at
import time". `map file.py#` lists `__module__` first when the region is not
empty, so you can see whether it is there before asking for it. A file whose
first line is a definition has no preamble: that is exit `1`, not an error.

### The line-span escape hatch

`file.py#L10-L20` exists for the cases symbol addressing cannot reach: a region
of a file with no symbol boundary that matches what you want.

Reach for it last. A line span is exactly the brittle thing this tool exists to
avoid — it breaks the moment anything above it changes. If you find yourself
using line spans routinely, the symbol address you actually wanted probably
exists and you have not found it. Run `map` on the file first.

**Do not use a line span for imports.** `#L1-L40` for a file preamble is the
single most common instance of this mistake, and `#__module__` addresses it
exactly, without needing to know where the first definition starts.

### Forms that are accepted and rewritten

These are tolerated so a near-miss does not cost a turn. **They are not the
idiom** — write the canonical form above. Each prints a `note:` on stderr
naming what it read, and rewriting happens only when the file exists, so
nothing is guessed.

```
file.py                 ->  file.py#          (the outline)
file.py 10 20           ->  file.py#L10-L20
file.py:10-20           ->  file.py#L10-L20
file.py::Symbol         ->  file.py#Symbol
file.py:10-20#Symbol    ->  file.py#Symbol    (the line range is redundant)
```

So `slice file.py` does work, and returns the outline. Prefer `file.py#`,
which says what you mean without relying on the rewrite.

A bare name with no `#` at all is resolved against the symbol index:
`slice SecurityMiddleware` finds the definition when the name is unique, and
exits `1` listing the candidates when it is not.

## `doclens` — json, yaml, markdown

Addressing differs by format because the underlying structures differ.

```
config.json#scripts             object key
config.json#scripts.build       nested key
compose.yaml#services.web       nested mapping
compose.yaml#services.web.ports[1]   array index, zero-based
README.md#Install               top-level heading
README.md#Install/From source   heading path, / between levels
```

JSON and YAML use `.` between keys and `[n]` for array indices. Markdown uses
`/` between heading levels, because `.` appears freely in heading text.

### Quoting

A key containing a dot must be quoted, or it reads as two steps:

```
config.json#["a.b"].c           the key "a.b", then key "c"
config.json#a.b.c               key "a", then "b", then "c"
```

Use double quotes inside the brackets. This is the single most common address
mistake with `doclens` — a `1` exit on a key you can see in the file usually
means an unquoted dot.

### Markdown heading paths

Heading text is matched as written, minus leading `#` characters and trailing
`#` padding. Nesting follows heading level, not document order, so a `###` under
an `##` is a child regardless of what sits between them. Fenced code blocks are
skipped, so a `# comment` inside a shell example is not mistaken for a heading.

## When an address misses

Exit code `1` with a list of what is actually present. Read that list rather
than guessing again — it is the cheapest correction available, and it is why
`map` is rarely needed as a separate step after a failed `slice`.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | found |
| `1` | not found, or not applicable |
| `2` | genuine tool fault |

**A mistyped address is `1`, not `2`.** So are a missing file, a missing
symbol, an empty module preamble, and an unsupported language or format —
`map notes.txt` is "not applicable", not a failure. Reserve `2` for a real
fault: an unreadable file, a parse failure, a bad `--kind` or `--match` value.

Branch on this. A `1` means rephrase the address or accept the answer is "not
there". A `2` means the tool could not do its job and retrying the same call
will not help.

Under `--json`, errors are a JSON object on **stdout** with the same envelope
shape as success — `schema_version`, `tool_version`, `command`, `found: false`,
and an `error` object — so a failed call is still parseable. The exit code is
unchanged. You never need to parse stderr.
