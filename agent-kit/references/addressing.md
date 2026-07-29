# Address grammar

Everything is addressed symbolically and re-resolved on every call. Line numbers
are supplementary output, never the address. This is the whole point: an address
survives edits above it, a line number does not.

## `agentlens` — code

```
file.py#Symbol                  top-level function or class
file.py#Class.method            method on a class
file.py#Class.Inner.method      arbitrary nesting
file.py#                        outline of the file, no body
file.py#L10-L20                 literal line span
```

The `#` is required. `file.py` alone is a path, not an address — `map` accepts
it, `slice` does not.

### The line-span escape hatch

`file.py#L10-L20` exists for the cases symbol addressing cannot reach: module-
level code outside any definition, a config block, a region of a file with no
symbol boundary that matches what you want.

Reach for it last. A line span is exactly the brittle thing this tool exists to
avoid — it breaks the moment anything above it changes. If you find yourself
using line spans routinely, the symbol address you actually wanted probably
exists and you have not found it. Run `map` on the file first.

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
