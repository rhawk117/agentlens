#!/usr/bin/env bash
# PostToolUse hook. Runs the gating clippy pass after a Rust file is written and
# blocks on violations. Flags mirror scripts/lint.sh exactly, so a pass here
# means that stage of the gate will also pass.
set -uo pipefail

readonly FEEDBACK_LINES=60

file="$(jq -r '.tool_response.filePath // .tool_input.file_path // empty' 2>/dev/null || true)"
case "${file}" in
    *.rs) ;;
    *) exit 0 ;;
esac

repo="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null || true)}"
[[ -n ${repo} && -d ${repo} ]] || exit 0
cd "${repo}" || exit 0

output="$(cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::nursery 2>&1)"
status=$?
if ((status != 0)); then
    {
        echo "clippy failed after editing ${file}:"
        echo
        echo "${output}" | tail -n "${FEEDBACK_LINES}"
    } >&2
    exit 2
fi
