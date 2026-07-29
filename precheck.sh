#!/usr/bin/env bash
# Contributor bootstrap. Verifies the toolchain this repo needs and reports what
# is missing, with the command to fix it. Assumes nothing is installed.
# Pass --install to install what is missing.
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/log.sh
source "${REPO_ROOT}/scripts/log.sh"
cd "${REPO_ROOT}"

do_install=false
for arg in "$@"; do
    case "${arg}" in
        --install) do_install=true ;;
        *) log_error "Unknown argument: ${arg}"; exit 2 ;;
    esac
done

missing=()

log_step "precheck: rust toolchain"
if ! command -v rustup >/dev/null 2>&1; then
    log_error "rustup is not installed"
    log_info "install it with:"
    log_info "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    log_info "then re-run ./precheck.sh"
    log_step_end
    exit 1
fi
log_success "rustup present"

# rust-toolchain.toml pins the version; this materializes it.
log_info "syncing pinned toolchain from rust-toolchain.toml"
rustup show active-toolchain >/dev/null
log_success "toolchain: $(rustc --version)"
log_step_end

log_step "precheck: cargo tooling"
# "name|what it is for". Not an associative array: this script runs before the
# contributor has installed anything, and macOS still ships bash 3.2, which has
# neither `declare -A` nor a defined iteration order.
tools=(
    "cargo-nextest|test runner"
    "cargo-deny|supply-chain audit"
    "cargo-machete|unused dependency detection"
    "cargo-llvm-cov|coverage"
)
for entry in "${tools[@]}"; do
    tool="${entry%%|*}"
    purpose="${entry#*|}"
    if command -v "${tool}" >/dev/null 2>&1; then
        log_success "${tool} (${purpose})"
    else
        log_warn "${tool} missing (${purpose})"
        missing+=("${tool}")
    fi
done
log_step_end

log_step "precheck: pre-commit"
if command -v pre-commit >/dev/null 2>&1; then
    log_success "pre-commit present"
    if [[ -f .git/hooks/pre-commit ]]; then
        log_success "git hook installed"
    else
        log_warn "git hook not installed; run: pre-commit install"
    fi
else
    log_warn "pre-commit missing"
    log_info "install with: pipx install pre-commit  (or: uv tool install pre-commit)"
fi
log_step_end

if (( ${#missing[@]} > 0 )); then
    if [[ ${do_install} == true ]]; then
        log_step "precheck: installing ${#missing[@]} tool(s)"
        if command -v cargo-binstall >/dev/null 2>&1; then
            cargo binstall --no-confirm "${missing[@]}"
        else
            log_info "cargo-binstall not found; building from source (slow)"
            cargo install --locked "${missing[@]}"
        fi
        log_step_end
        log_success "tooling installed"
    else
        log_warn "${#missing[@]} tool(s) missing. Install with:"
        log_info "  ./precheck.sh --install"
        log_info "or manually: cargo install --locked ${missing[*]}"
        exit 1
    fi
fi

log_success "environment ready — run scripts/ci.sh to verify"
