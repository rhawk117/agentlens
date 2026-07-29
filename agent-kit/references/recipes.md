# Recipes

Task-to-command patterns. Each shows the command, and where it is not obvious,
what it saves over the alternative.

## Worked trajectories

Complete paths from question to answer. The failure mode these exist to prevent
is `map`-ing your way toward a symbol whose name you were already given.

### "Where is retry backoff computed, and what is the cap?"

The question names the thing. Two commands:

```
agentlens find backoff --kind definition
# -> http/client.py#Client._backoff

agentlens packet http/client.py#Client._backoff
# -> the body, its callers, its callees, all at once
```

What not to do, and why it is tempting: `map` the directory, then `map` each
file that looks relevant, then `slice` the method once you spot it. That is four
to six calls and several outlines of context to reach the same two facts. The
name in the question was already enough to skip all of it.

### "Which functions call the backoff function?"

Already answered by the `packet` above. If you ran a bare `slice` instead:

```
agentlens callers http/client.py#Client._backoff
```

One call. Do not re-`map` the tree to find callers by eye, and do not grep for
the name — grep cannot tell a call from an import or a mention in a docstring,
which is the distinction the question is asking about.

### "Is there any dead code here?"

```
agentlens dead --root .
```

Then verify each candidate before reporting it:

```
agentlens callers <candidate-address>
```

`dead` reports unreferenced **functions and classes**. It does not analyse
unused imports or unused constants, so do not attribute those to it. Public API
classes with no in-tree callers will appear and are usually live; say so rather
than listing them as findings.

### "How does this repo fit together?"

Here you genuinely cannot name anything, so orientation is correct:

```
agentlens map . --depth 2
agentlens map src/api/users.py
agentlens packet src/api/users.py#UserService.create_user
```

Three calls, each narrowing. Note the shape: `map` appears at the start, once,
and stops as soon as you have a name to address.

## Orient in an unfamiliar repo

```
agentlens map . --depth 2
```

Languages, file and symbol counts, and the shape of the tree. Start here rather
than with `ls -R` or reading a README, then narrow to a file.

## Understand one function before changing it

```
agentlens packet src/http/client.py#Client.request
```

Returns the body, the signatures of everything it calls, and the signatures of
everything that calls it. This is the single highest-value command in the tool:
the alternative is a `slice`, a `callers`, and a `slice` per callee — five to
ten calls and several files of context.

Use plain `slice` when you only need the body:

```
agentlens slice src/http/client.py#Client.request
agentlens slice src/http/client.py#Client.request --signature-only
```

## Find where something is defined

```
agentlens find Client --kind definition
agentlens find "^_backoff$" --kind definition --exact
```

`--kind definition` is what separates this from grep: it excludes call sites,
imports, and mentions in comments, so one command answers "where does this
live" without a page of noise.

## Assess a change's blast radius

```
agentlens callers src/http/client.py#Client._backoff
agentlens callers src/http/client.py#Client._backoff --no-tests
```

`--no-tests` answers "what production code breaks", which is usually the real
question. Read the confidence tier before treating a hit as a definite call
site.

## Audit configuration and magic values

```
agentlens literals src/ --kind string --min-len 8
agentlens literals --in src/http/client.py#Client --match "https?://"
```

`--in` scopes extraction to one symbol's body. Values are grouped by default, so
a string appearing in nine places is reported once with nine locations rather
than nine times.

## Look for dead code

```
agentlens dead --root .
```

Candidates, not conclusions — see the warning in `commands.md`. Confirm each one
with `callers` before proposing a deletion, and remember that entry points,
plugin hooks, and anything invoked by name at runtime will show up here while
being entirely live.

## Pull one value out of config

```
doclens slice compose.yaml#services.web.ports[1]
doclens slice package.json#scripts.build
doclens slice pyproject.toml#tool.ruff --with-key
```

Byte spans, not re-serialised values, so comments and key order survive. Use
`--with-key` when the surrounding key line is part of what you need to show.

## Read one section of a long markdown file

```
doclens map README.md --depth 2
doclens slice README.md#Install/From source
```

The `map` first is worth it on a document you have not seen: heading text has to
match, and the outline gives you the exact strings.

## Search config across a tree

```
doclens find postgres --values
doclens find --keys "^DATABASE_"
```

`--keys` and `--values` are the useful discriminator. Searching keys finds
structure; searching values finds data. Searching both usually finds noise.

## Keep output small

Every command takes `--budget <n>`, default 4000. Lowering it degrades the
output — fewer results, shorter previews — rather than cutting it off mid-token.

```
agentlens map src/ --depth 3 --budget 1500
```

Pair with `--quiet` to drop the summary line when you only want the payload.
