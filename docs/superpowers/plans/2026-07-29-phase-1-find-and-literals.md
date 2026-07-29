# Phase 1 Migration: find, literals, and the shared Matcher

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the phase 1 snapshot — the `find` and `literals` commands plus the shared `Matcher` — on `dev`, adapted to `dev`'s stricter lint configuration.

**Architecture:** The snapshot's source is copied in as-is; the work is adapting it to a stricter gate than it was written against. `matcher.rs` provides pattern matching shared by `ops/find.rs` and `ops/literals.rs`; both are wired through `ops/mod.rs` and the CLI. Supporting modules (`address`, `error`, `render`, `source`, `walk`, `ops/map`, `ops/slice`) gain the surface these two ops need.

**Tech Stack:** Rust, edition 2024, MSRV 1.97. `tree-sitter` + `tree-sitter-python`, `clap`, `serde`/`serde_json`, `regex`, `walkdir`. Tests via `cargo-nextest` with text snapshot fixtures.

## Global Constraints

- **Frozen paths — do not modify:** `Cargo.toml`, `crates/*/Cargo.toml`, `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.github/workflows/**`, `scripts/**`, `precheck.sh`, `.gitignore`, `rust-toolchain.toml`.
- **The snapshot's own config is older and weaker than `dev`'s and must never be copied in.** Snapshot is edition 2021 / MSRV 1.82 with `pedantic = warn`; `dev` is edition 2024 / MSRV 1.97 with `pedantic = deny` plus `unwrap_used`, `expect_used`, `panic`, and `indexing_slicing` all denied.
- **No dependency changes.** `regex` and `walkdir` are already in `dev`'s `[workspace.dependencies]`.
- **Fix failures in source only.** Never relax the gate, delete a test, or add a crate-wide `allow` to reach green.
- Verification gate: `bash scripts/ci.sh` (`format.sh --check`, `lint.sh`, `build.sh`, `test.sh`, `audit.sh`).
- Branch `feat/find-and-literals`, already created off `dev` at `63c7119`. One squash-merged PR.

## Established facts

Investigated on 2026-07-29; do not re-derive.

- Snapshot extracted at `/tmp/agentlensphase1/agentlens-phase1/` (78 files).
- Source and test files are already copied onto the branch. Frozen paths verified untouched via `git status --porcelain -- Cargo.toml rustfmt.toml .github scripts .gitignore crates/*/Cargo.toml` returning empty.
- `budget.rs`, `lang.rs`, `symbols.rs`, and `tests/fixtures/**` are byte-identical to `dev` — not copied, not touched.
- Snapshot test fixtures went 20 → 46 files.
- **Edition 2024 requires no source migration.** The snapshot compiles under edition 2024; every failure below is a lint error, not a compile error.
- `bash scripts/lint.sh` exits 1 with **exactly 16 errors, all in `agentlens-core`**. The CLI crate has not been linted yet, because core fails first — Task 6 exists to catch anything downstream.

The 16 violations:

| Lint | Count | Locations |
|---|---|---|
| `unused_qualifications` | 2 | `walk.rs:62`, `walk.rs:86` |
| `clippy::missing_errors_doc` | 8 | `address.rs:20`, `matcher.rs:12`, `ops/find.rs:123`, `ops/literals.rs:120`, `ops/map.rs:58`, `ops/slice.rs:47`, `source.rs:19`, `source.rs:28` |
| `clippy::collapsible_if` | 3 | `ops/find.rs:320`, `ops/find.rs:325`, `ops/find.rs:413` |
| `clippy::struct_excessive_bools` | 2 | `ops/find.rs:83`, `ops/literals.rs:68` |
| `clippy::indexing_slicing` | 1 | `source.rs:136` |

Line numbers are from the snapshot as copied. Fix tasks in ascending file order per task to keep them stable; re-run lint after each task to re-anchor.

## Not investigated

- The CLI crate's own lint cleanliness (blocked behind core). Task 6 surfaces it.
- Whether the 46 snapshot fixtures match this build's actual output. Task 7 runs the tests.
- `README.md` differs by 132 diff lines; Task 8 merges rather than overwrites.

---

### Task 1: Remove redundant `Result` qualification in `walk.rs`

`Result` is in the prelude, so `std::result::Result::ok` is a redundant path. `dev` sets `unused_qualifications = "warn"` and `lint.sh` passes `-D warnings`.

**Files:**
- Modify: `crates/agentlens-core/src/walk.rs:62`, `crates/agentlens-core/src/walk.rs:86`

- [ ] **Step 1: Confirm the failure**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'unnecessary qualification'`
Expected: `2`

- [ ] **Step 2: Apply the fix at both sites**

Both lines are currently:

```rust
        .filter_map(std::result::Result::ok)
```

Change each to:

```rust
        .filter_map(Result::ok)
```

- [ ] **Step 3: Verify the lint is gone**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'unnecessary qualification'`
Expected: `0`

- [ ] **Step 4: Commit**

```bash
git add crates/agentlens-core/src/walk.rs
git commit -m "refactor(walk): drop redundant Result path qualification"
```

---

### Task 2: Replace panicking index with `last()` in `source.rs`

`indexing_slicing` is denied because a bare index turns a possible runtime panic into something the compiler cannot see. `.last()` expresses the same intent and cannot panic.

**Files:**
- Modify: `crates/agentlens-core/src/source.rs:136`

- [ ] **Step 1: Confirm the failure**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'indexing may panic'`
Expected: `1`

- [ ] **Step 2: Apply the fix**

Current:

```rust
    if starts.len() > 1 && starts[starts.len() - 1] == text.len() {
        starts.pop();
    }
```

Replace with:

```rust
    if starts.len() > 1 && starts.last() == Some(&text.len()) {
        starts.pop();
    }
```

`starts.last()` yields `Option<&usize>`, so the comparison is against `Some(&text.len())`. The `starts.len() > 1` guard is retained: it is a real precondition, not a bounds check, and dropping it would change behavior for a single-element `starts`.

- [ ] **Step 3: Verify**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'indexing may panic'`
Expected: `0`

- [ ] **Step 4: Confirm line-mapping behavior is unchanged**

Run: `cargo nextest run -p agentlens-core source::`
Expected: PASS, including `source::tests::maps_bytes_to_lines` and `source::tests::counts_lines`

- [ ] **Step 5: Commit**

```bash
git add crates/agentlens-core/src/source.rs
git commit -m "refactor(source): use last() instead of a panicking index"
```

---

### Task 3: Collapse nested conditionals in `ops/find.rs`

Three nested `if`s that edition 2024 let-chains express directly. Let-chains are stable in edition 2024 on MSRV 1.97, so this is available here even though the snapshot (edition 2021) could not use it.

**Files:**
- Modify: `crates/agentlens-core/src/ops/find.rs` around lines 320, 325, 413

- [ ] **Step 1: Confirm the failure**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'this .if. statement can be collapsed'`
Expected: `3`

- [ ] **Step 2: Collapse the attribute-call chain (lines ~320-330)**

Current:

```rust
    if parent.kind() == "attribute"
        && parent
            .child_by_field_name("attribute")
            .is_some_and(|attribute| attribute.id() == node.id())
    {
        if let Some(grand) = parent.parent() {
            if grand.kind() == "call"
                && grand
                    .child_by_field_name("function")
                    .is_some_and(|function| function.id() == parent.id())
            {
```

Replace the three-level nest with a single let-chain:

```rust
    if parent.kind() == "attribute"
        && parent
            .child_by_field_name("attribute")
            .is_some_and(|attribute| attribute.id() == node.id())
        && let Some(grand) = parent.parent()
        && grand.kind() == "call"
        && grand
            .child_by_field_name("function")
            .is_some_and(|function| function.id() == parent.id())
    {
```

Remove the two now-unmatched closing braces that previously closed the inner `if`s. Keep the body unchanged.

- [ ] **Step 3: Collapse the truncation-note guard (lines ~413-418)**

Current:

```rust
    if capped {
        if let Some(note) = truncation_note(hit_count, HARD_CAP + 1, "narrow the pattern or paths")
        {
            out.push(note);
        }
    }
```

Replace with:

```rust
    if capped
        && let Some(note) = truncation_note(hit_count, HARD_CAP + 1, "narrow the pattern or paths")
    {
        out.push(note);
    }
```

- [ ] **Step 4: Verify**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'this .if. statement can be collapsed'`
Expected: `0`

- [ ] **Step 5: Confirm occurrence classification is unchanged**

Run: `cargo nextest run -p agentlens-cli`
Expected: the `find_*` snapshot tests pass. If `find_exact_calls` or `find_definitions` fail here, the collapse changed semantics — revert Step 2 and redo it one nesting level at a time.

- [ ] **Step 6: Commit**

```bash
git add crates/agentlens-core/src/ops/find.rs
git commit -m "refactor(find): collapse nested conditionals into let-chains"
```

---

### Task 4: Document error conditions on fallible public functions

`dev` removed `missing_errors_doc` from the allow list, so every public `fn` returning `Result` needs an `# Errors` section. These are real doc improvements, not lint appeasement — each says which `Error` variant callers can expect.

**Files:**
- Modify: `crates/agentlens-core/src/address.rs:20`, `matcher.rs:12`, `ops/find.rs:123`, `ops/literals.rs:120`, `ops/map.rs:58`, `ops/slice.rs:47`, `source.rs:19`, `source.rs:28`

**Interfaces:** No signature changes. Doc comments only.

- [ ] **Step 1: Confirm the failure**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'missing .# Errors. section'`
Expected: `8`

- [ ] **Step 2: Add each doc block**

Insert immediately above each function. Read the function body first and name the variants it actually returns — do not copy a generic sentence.

`address.rs`, above `pub fn parse`:

```rust
    /// Parse a `path#selector` address.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AddressMissingHash`] if `raw` has no `#`,
    /// [`Error::AddressEmptyPath`] if the path before `#` is empty, and
    /// [`Error::BadLineSpan`] if a line-span selector is malformed.
```

`matcher.rs`, above `pub fn new`:

```rust
    /// Build a matcher for `pattern`, literal when `exact` is set and a regex otherwise.
    ///
    /// # Errors
    ///
    /// Returns [`Error::BadRegex`] if `exact` is false and `pattern` is not a valid regex.
```

`source.rs`, above `pub fn load`:

```rust
    /// Load and parse a source file from disk.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedLanguage`] if the extension maps to no known
    /// language, [`Error::Io`] if `path` cannot be read, and [`Error::NotUtf8`]
    /// if its contents are not valid UTF-8.
```

`source.rs`, above `pub fn from_text`:

```rust
    /// Parse already-loaded source text.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Parse`] if the tree-sitter parser cannot produce a tree.
```

`ops/find.rs`, above `pub fn run`:

```rust
/// Find occurrences of `pattern` across `paths`.
///
/// # Errors
///
/// Returns [`Error::BadRegex`] if `pattern` is not a valid regex and
/// `options.exact` is unset, and propagates [`Error::Io`], [`Error::NotUtf8`],
/// and [`Error::Parse`] from the files it visits.
```

`ops/literals.rs`, above `pub fn run`:

```rust
/// Extract string and numeric literals from `paths`.
///
/// # Errors
///
/// Returns [`Error::BadRegex`] if `options.match_pattern` is not a valid
/// regex, and propagates [`Error::Io`], [`Error::NotUtf8`], and
/// [`Error::Parse`] from the files it visits.
```

`ops/map.rs`, above `pub fn run`:

```rust
/// Produce a structural map of a file or directory.
///
/// # Errors
///
/// Propagates [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`] from
/// the files it visits.
```

`ops/slice.rs`, above `pub fn run`:

```rust
/// Extract the source slice a resolved address points at.
///
/// # Errors
///
/// Propagates [`Error::Io`], [`Error::NotUtf8`], and [`Error::Parse`] from the
/// addressed file. A selector matching no symbol is reported in the returned
/// [`Report`], not as an error.
```

If a referenced variant name does not exist, check `crates/agentlens-core/src/error.rs` and use the real one. Do not invent variants to satisfy the doc.

- [ ] **Step 3: Verify**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'missing .# Errors. section'`
Expected: `0`

- [ ] **Step 4: Confirm the doc links resolve**

Run: `cargo doc -p agentlens-core --no-deps 2>&1 | grep -i 'unresolved link' || echo "no broken links"`
Expected: `no broken links`

- [ ] **Step 5: Commit**

```bash
git add crates/agentlens-core/src
git commit -m "docs(core): document error conditions on fallible public functions"
```

---

### Task 5: Exempt the two option structs from `struct_excessive_bools`

`FindOptions` and `LiteralsOptions` each carry four independent `bool` flags mirroring CLI switches. The lint's suggested remedy — folding them into a state enum — would misrepresent flags that genuinely combine freely, and would change public API for no correctness gain.

This is the one place the plan suppresses rather than fixes. It is a targeted item-level `#[allow]` with a stated reason, not a crate-wide or config-level relaxation, so every other struct still gets the lint.

**Files:**
- Modify: `crates/agentlens-core/src/ops/find.rs:82`, `crates/agentlens-core/src/ops/literals.rs:67`

- [ ] **Step 1: Confirm the failure**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'more than 3 bools'`
Expected: `2`

- [ ] **Step 2: Annotate `FindOptions`**

Current:

```rust
#[derive(Debug, Clone)]
pub struct FindOptions {
```

Replace with:

```rust
// Four independent CLI switches. Collapsing them into an enum would imply
// mutual exclusivity that does not exist: any combination is valid.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct FindOptions {
```

- [ ] **Step 3: Annotate `LiteralsOptions`**

```rust
// Four independent CLI switches; see the note on FindOptions.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
pub struct LiteralsOptions {
```

- [ ] **Step 4: Verify**

Run: `bash scripts/lint.sh 2>&1 | grep -c 'more than 3 bools'`
Expected: `0`

- [ ] **Step 5: Confirm core now lints clean**

Run: `bash scripts/lint.sh`
Expected: exit 0, or failures only in `agentlens-cli` — which Task 6 handles.

- [ ] **Step 6: Commit**

```bash
git add crates/agentlens-core/src/ops/find.rs crates/agentlens-core/src/ops/literals.rs
git commit -m "chore(core): exempt option structs from struct_excessive_bools"
```

---

### Task 6: Clear any violations in the CLI crate

Core failing first masked the CLI crate. This task exists because the violation list above is provably incomplete, not because specific failures are known.

**Files:**
- Modify: `crates/agentlens-cli/src/main.rs`, `crates/agentlens-cli/tests/cli.rs` as needed

- [ ] **Step 1: Lint the whole workspace**

Run: `bash scripts/lint.sh`
Expected: exit 0. If it exits 0, mark this task complete and move on.

- [ ] **Step 2: Fix each reported violation in source**

Apply the same rules as Tasks 1-5: prefer a real fix; use a targeted item-level `#[allow]` with a written justification only when the lint's remedy would harm the design. Never edit `Cargo.toml` or `clippy.toml`.

- [ ] **Step 3: Verify**

Run: `bash scripts/lint.sh`
Expected: exit 0

- [ ] **Step 4: Commit (skip if Step 1 was already clean)**

```bash
git add crates/agentlens-cli
git commit -m "fix(cli): satisfy workspace lint configuration"
```

---

### Task 7: Full gate green

**Files:** none expected; fixes land wherever the gate points.

- [ ] **Step 1: Check formatting**

Run: `bash scripts/format.sh --check`
Expected: exit 0. If it fails, run `bash scripts/format.sh` and re-check — `rustfmt.toml` is frozen, so the source conforms to it, never the reverse.

- [ ] **Step 2: Run the tests**

Run: `bash scripts/test.sh`
Expected: all tests pass, including the 26 new `find_*` and `literals_*` snapshot tests.

If a snapshot test fails, diff expected against actual before touching anything. A fixture may legitimately need regenerating if `dev` changed rendering, but a mismatch is equally likely to be a real behavioral difference introduced by Tasks 2 or 3. Do not regenerate a fixture to make a test pass without first explaining why the old expectation was wrong.

- [ ] **Step 3: Run the full gate**

Run: `bash scripts/ci.sh`
Expected: `gate passed`

- [ ] **Step 4: Confirm no frozen path was touched**

Run:

```bash
git diff --name-only dev... -- Cargo.toml 'crates/*/Cargo.toml' rustfmt.toml clippy.toml deny.toml .github scripts precheck.sh .gitignore rust-toolchain.toml
```

Expected: empty output. Any path listed here must be reverted with `git checkout dev -- <path>` and the gate re-run.

- [ ] **Step 5: Commit any fixes**

```bash
git add -A
git commit -m "test: bring phase 1 suite green under the workspace gate"
```

---

### Task 8: Merge the README

The snapshot's `README.md` differs by 132 diff lines. It documents `find` and `literals`, but `dev`'s copy has content the snapshot predates. Overwriting would drop that.

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Inspect the difference**

Run: `diff -u README.md /tmp/agentlensphase1/agentlens-phase1/README.md`

- [ ] **Step 2: Port only the additive parts**

Copy across the `find` and `literals` documentation. Keep `dev`'s existing sections, its wording, and anything describing tooling the snapshot does not know about. Do not reintroduce snapshot references to edition 2021 or MSRV 1.82.

- [ ] **Step 3: Verify no stale version claims remain**

Run: `grep -nE '2021|1\.82' README.md || echo "clean"`
Expected: `clean`

- [ ] **Step 4: Commit**

```bash
git add README.md
git commit -m "docs: document the find and literals commands"
```

---

### Task 9: Open the PR and merge

- [ ] **Step 1: Final gate**

Run: `bash scripts/ci.sh`
Expected: `gate passed`

- [ ] **Step 2: Push**

```bash
git push -u origin feat/find-and-literals
```

- [ ] **Step 3: Open the PR**

```bash
gh pr create --base dev --head feat/find-and-literals \
  --title "feat(core): add find and literals commands" \
  --body-file <path to a written body file>
```

Write the body to a file and pass `--body-file`. A `cat <<EOF` heredoc will fail in this environment: `cat` is aliased to `bat`, which is not installed. If `gh pr edit` later fails on a Projects-classic GraphQL error, set the body with
`gh api -X PATCH repos/:owner/:repo/pulls/<n> -F body=@<file>`.

- [ ] **Step 4: Record the branch in `LOOP_STATE.md`**

Add the line `current_branch: feat/find-and-literals`. `scripts/advance-phase.sh` reads it to confirm the phase merged, and refuses without it.

- [ ] **Step 5: Watch CI**

Dispatch the `ci-watcher` subagent with the PR number. It returns `PASS`, or `FAIL` plus failing job logs, then exits. On `FAIL`, fix in source, push, and dispatch a **new** `ci-watcher`. Cap at 3 attempts, then stop and surface the blocker.

- [ ] **Step 6: Merge**

```bash
gh pr merge <n> --squash --delete-branch
git checkout dev && git pull --ff-only
```

- [ ] **Step 7: Stop**

Do not run `scripts/advance-phase.sh`. The user's standing constraint is to stop after phase 1 so iteration 1 can be verified end to end.
