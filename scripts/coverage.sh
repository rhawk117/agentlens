#!/usr/bin/env bash
# Generate a coverage report. Not part of the CI gate (slow instrumented rebuild).
# Pass --html to open a browsable report, --lcov to emit lcov.info.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

if ! cargo llvm-cov --version >/dev/null 2>&1; then
    log_error "cargo-llvm-cov not installed"
    log_info "install with: cargo install --locked cargo-llvm-cov"
    exit 1
fi

output_args=(--summary-only)
for arg in "$@"; do
    case "${arg}" in
        --html) output_args=(--html --open) ;;
        --lcov) output_args=(--lcov --output-path lcov.info) ;;
        *) log_error "Unknown argument: ${arg}"; exit 2 ;;
    esac
done

log_step "coverage: cargo llvm-cov"
cargo llvm-cov --workspace --locked "${output_args[@]}"
log_step_end
log_success "coverage report generated"
