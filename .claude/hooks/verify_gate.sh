#!/usr/bin/env bash
# Stop hook. Blocks the end of a turn when the working tree carries Rust or
# manifest changes and scripts/ci.sh --fast does not pass. Exit 2 feeds the gate
# output back to the agent so it fixes the tree instead of handing over a red one.
#
# Conversational turns stay free: with no Rust/manifest changes staged or
# unstaged, this exits 0 without invoking cargo at all.
set -uo pipefail

readonly FEEDBACK_LINES=80

repo="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || true)}"
[[ -n ${repo} && -d ${repo} ]] || exit 0
cd "${repo}" || exit 0

# Pathspecs cover tracked and untracked files; git's `*` spans directories.
if ! git status --porcelain -- '*.rs' '*.toml' 'Cargo.lock' 2>/dev/null | grep -q .; then
    exit 0
fi

output="$(scripts/ci.sh --fast 2>&1)"
status=$?
if ((status != 0)); then
    {
        echo "scripts/ci.sh --fast failed. Fix the tree before ending the turn."
        echo
        echo "${output}" | tail -n "${FEEDBACK_LINES}"
    } >&2
    exit 2
fi
