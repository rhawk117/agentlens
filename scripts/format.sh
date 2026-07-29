#!/usr/bin/env bash
# Format Rust sources. Pass --check to verify without writing (CI mode).
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

check_mode=false
for arg in "$@"; do
    case "${arg}" in
        --check) check_mode=true ;;
        *) log_error "Unknown argument: ${arg}"; exit 2 ;;
    esac
done

if [[ ${check_mode} == true ]]; then
    log_step "format: checking"
    if cargo fmt --all --check; then
        log_step_end
        log_success "formatting is clean"
    else
        log_step_end
        log_error "formatting violations found; run scripts/format.sh to fix"
        exit 1
    fi
else
    log_step "format: writing"
    cargo fmt --all
    log_step_end
    log_success "formatted"
fi
