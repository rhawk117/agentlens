#!/usr/bin/env bash
# Full gate. Runs the same checks locally and in GitHub Actions.
# Pass --fast to skip the build and audit stages.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"
# shellcheck source=scripts/log.sh
source "${SCRIPT_DIR}/log.sh"
cd "${REPO_ROOT}"

fast=false
for arg in "$@"; do
    case "${arg}" in
        --fast) fast=true ;;
        *) log_error "Unknown argument: ${arg}"; exit 2 ;;
    esac
done

stages=("format.sh --check" "lint.sh")
if [[ ${fast} == false ]]; then
    stages+=("build.sh" "test.sh" "audit.sh")
else
    stages+=("test.sh")
fi

failed=()
for stage in "${stages[@]}"; do
    # shellcheck disable=SC2086
    if ! "${SCRIPT_DIR}"/${stage}; then
        failed+=("${stage}")
    fi
done

if (( ${#failed[@]} > 0 )); then
    log_error "gate failed: ${failed[*]}"
    exit 1
fi

log_success "gate passed"
