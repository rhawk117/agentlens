#!/usr/bin/env bash
# Supply-chain gate: security advisories, license policy, banned and duplicate crates.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

if ! cargo deny --version >/dev/null 2>&1; then
    log_warn "cargo-deny not installed; skipping supply-chain audit"
    log_info "install with: cargo install --locked cargo-deny"
    exit 0
fi

log_step "audit: cargo deny"
if ! cargo deny check; then
    log_step_end
    log_error "supply-chain audit failed"
    exit 1
fi
log_step_end
log_success "supply-chain audit passed"
