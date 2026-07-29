#!/usr/bin/env bash
# Build the workspace. Pass --release for an optimized build.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

profile_args=()
for arg in "$@"; do
    case "${arg}" in
        --release) profile_args+=(--release) ;;
        *) log_error "Unknown argument: ${arg}"; exit 2 ;;
    esac
done

log_step "build: cargo build ${profile_args[*]-}"
cargo build --workspace --all-targets --locked "${profile_args[@]}"
log_step_end
log_success "build succeeded"
