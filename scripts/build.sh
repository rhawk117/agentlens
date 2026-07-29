#!/usr/bin/env bash
# Build the workspace. Pass --release for an optimized build.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

require_cmd cargo

# Seeded with the fixed flags rather than collecting only the optional ones:
# expanding an empty array under `set -u` is an error in bash 3.2, which is what
# macOS ships.
build_args=(--workspace --all-targets --locked)
for arg in "$@"; do
    case "${arg}" in
        --release) build_args+=(--release) ;;
        *) log_error "Unknown argument: ${arg}"; exit 2 ;;
    esac
done

log_step "build: cargo build $*"
cargo build "${build_args[@]}"
log_step_end
log_success "build succeeded"
