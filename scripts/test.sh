#!/usr/bin/env bash
# Run the test suite. Uses cargo-nextest when available, otherwise cargo test.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

if cargo nextest --version >/dev/null 2>&1; then
    log_step "test: nextest"
    cargo nextest run --workspace --locked
    log_step_end
    # nextest does not execute doctests; run them separately.
    log_step "test: doctests"
    cargo test --workspace --locked --doc
    log_step_end
else
    log_warn "cargo-nextest not installed; falling back to cargo test"
    log_step "test: cargo test"
    cargo test --workspace --locked
    log_step_end
fi

log_success "tests passed"
