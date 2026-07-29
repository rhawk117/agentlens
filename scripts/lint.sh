#!/usr/bin/env bash
# Lint Rust sources. Gating pass denies all warnings except the nursery group,
# which is advisory only (nursery lints are in-development and may false-positive).
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

log_step "lint: clippy (gating)"
if ! cargo clippy --workspace --all-targets --locked -- -D warnings -A clippy::nursery; then
    log_step_end
    log_error "clippy found violations"
    exit 1
fi
log_step_end
log_success "clippy clean"

log_step "lint: clippy nursery (advisory)"
cargo clippy --workspace --all-targets --locked -- -A clippy::all -W clippy::nursery || true
log_step_end

if command -v cargo-machete >/dev/null 2>&1; then
    log_step "lint: unused dependencies"
    if ! cargo machete; then
        log_step_end
        log_error "unused dependencies declared in Cargo.toml"
        exit 1
    fi
    log_step_end
    log_success "no unused dependencies"
else
    log_warn "cargo-machete not installed; skipping unused-dependency check"
fi
