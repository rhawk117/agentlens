#!/bin/sh
# Install agentlens and doclens.
#
# Prefers a published release binary for the detected platform and falls back to
# building from source with cargo. No releases exist yet, so the source path is
# the live one today; the download path activates the first time a v* tag ships.
#
# Usage:
#   ./install.sh [--version <tag>] [--dir <path>] [--from-source]
#                [--yes] [--no-agent-kit] [--help]
#
#   curl -fsSL https://raw.githubusercontent.com/rhawk117/agentlens/dev/install.sh | sh

set -eu

REPO="rhawk117/agentlens"
API="https://api.github.com/repos/${REPO}"
DOWNLOADS="https://github.com/${REPO}/releases/download"

INSTALL_DIR="${AGENTLENS_INSTALL_DIR:-${HOME}/.local/bin}"
VERSION=""
FROM_SOURCE=false
ASSUME_YES=false
WANT_AGENT_KIT=true

# Resolved lazily so --help never touches the network or the filesystem.
SCRIPT_DIR=""
WORKDIR=""

# ---------------------------------------------------------------- reporting --

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_DIM=$(printf '\033[2m')
    C_RED=$(printf '\033[31m')
    C_GREEN=$(printf '\033[32m')
    C_YELLOW=$(printf '\033[33m')
    C_OFF=$(printf '\033[0m')
else
    C_DIM='' C_RED='' C_GREEN='' C_YELLOW='' C_OFF=''
fi

say() { printf '%s\n' "$*"; }
step() { printf '%s==>%s %s\n' "${C_DIM}" "${C_OFF}" "$*"; }
warn() { printf '%swarn:%s %s\n' "${C_YELLOW}" "${C_OFF}" "$*" >&2; }
ok() { printf '%sok:%s %s\n' "${C_GREEN}" "${C_OFF}" "$*"; }
die() {
    printf '%serror:%s %s\n' "${C_RED}" "${C_OFF}" "$*" >&2
    exit 1
}

have() { command -v "$1" >/dev/null 2>&1; }

usage() {
    cat <<'EOF'
Install agentlens and doclens.

  --version <tag>    Install a specific release (e.g. v0.2.0).
                     Default: the latest published release.
  --dir <path>       Install directory. Default: ~/.local/bin
                     (or $AGENTLENS_INSTALL_DIR).
  --from-source      Skip the release download and build with cargo.
  --yes              Answer yes to every prompt. Implied when stdin is
                     not a terminal.
  --no-agent-kit     Do not offer to install the skill and agent.
  --help             Show this message.
EOF
}

# ------------------------------------------------------------------- cleanup --

cleanup() {
    [ -n "${WORKDIR}" ] && [ -d "${WORKDIR}" ] && rm -rf "${WORKDIR}"
    return 0
}
trap cleanup EXIT INT TERM

# ------------------------------------------------------------------ platform --

# Emits a Rust target triple, or nothing when the platform has no published
# asset. An empty result routes to the source build; it is not a failure.
detect_target() {
    os=$(uname -s 2>/dev/null || echo unknown)
    arch=$(uname -m 2>/dev/null || echo unknown)

    case "${os}" in
        Linux) os_part="unknown-linux-gnu" ;;
        Darwin) os_part="apple-darwin" ;;
        MINGW* | MSYS* | CYGWIN*) os_part="pc-windows-msvc" ;;
        *) return 0 ;;
    esac

    case "${arch}" in
        x86_64 | amd64) arch_part="x86_64" ;;
        arm64 | aarch64) arch_part="aarch64" ;;
        *) return 0 ;;
    esac

    # Only these four are built. Linux ARM and 32-bit platforms fall through to
    # a source build rather than 404ing on a download.
    case "${arch_part}-${os_part}" in
        x86_64-unknown-linux-gnu | x86_64-apple-darwin | aarch64-apple-darwin | x86_64-pc-windows-msvc)
            printf '%s' "${arch_part}-${os_part}"
            ;;
        *) return 0 ;;
    esac
}

fetch() {
    if have curl; then
        curl -fsSL "$1" -o "$2"
    elif have wget; then
        wget -qO "$2" "$1"
    else
        die "need curl or wget to download; install one, or re-run with --from-source"
    fi
}

fetch_stdout() {
    if have curl; then
        curl -fsSL "$1" 2>/dev/null
    elif have wget; then
        wget -qO- "$1" 2>/dev/null
    else
        return 1
    fi
}

# Latest published tag, or nothing. A 404 here means no releases exist yet,
# which is an expected state and not an error.
resolve_version() {
    body=$(fetch_stdout "${API}/releases/latest") || return 0
    [ -n "${body}" ] || return 0
    printf '%s' "${body}" |
        grep -m1 '"tag_name"' |
        sed -e 's/.*"tag_name"[[:space:]]*:[[:space:]]*"//' -e 's/".*//'
}

verify_checksum() {
    archive=$1
    sums=$2

    if have sha256sum; then
        grep " \{1,2\}\.\{0,1\}/\{0,1\}${archive}\$" "${sums}" >expected.txt ||
            die "no checksum entry for ${archive}"
        sha256sum -c expected.txt >/dev/null 2>&1 ||
            die "checksum mismatch for ${archive}; refusing to install"
    elif have shasum; then
        actual=$(shasum -a 256 "${archive}" | cut -d' ' -f1)
        expected=$(grep "${archive}\$" "${sums}" | cut -d' ' -f1)
        [ -n "${expected}" ] || die "no checksum entry for ${archive}"
        [ "${actual}" = "${expected}" ] ||
            die "checksum mismatch for ${archive}; refusing to install"
    else
        warn "no sha256sum or shasum available; skipping checksum verification"
        return 0
    fi
    ok "checksum verified"
}

# ------------------------------------------------------------------- install --

install_from_release() {
    target=$1
    tag=$2

    case "${target}" in
        *windows*) archive="agentlens-${tag}-${target}.zip" ;;
        *) archive="agentlens-${tag}-${target}.tar.gz" ;;
    esac

    WORKDIR=$(mktemp -d 2>/dev/null || mktemp -d -t agentlens)
    step "downloading ${archive}"

    if ! fetch "${DOWNLOADS}/${tag}/${archive}" "${WORKDIR}/${archive}"; then
        warn "no release asset for ${target} at ${tag}"
        return 1
    fi
    fetch "${DOWNLOADS}/${tag}/SHA256SUMS" "${WORKDIR}/SHA256SUMS" ||
        die "downloaded ${archive} but SHA256SUMS is missing; refusing to install unverified binaries"

    (
        cd "${WORKDIR}"
        verify_checksum "${archive}" SHA256SUMS

        case "${archive}" in
            *.zip)
                have unzip || die "need unzip to extract ${archive}"
                unzip -q "${archive}"
                ;;
            *)
                tar xzf "${archive}"
                ;;
        esac
    )

    extracted="${WORKDIR}/agentlens-${tag}-${target}"
    [ -d "${extracted}" ] || die "archive layout unexpected: ${extracted} not found"

    mkdir -p "${INSTALL_DIR}"
    for bin in agentlens doclens; do
        for candidate in "${extracted}/${bin}" "${extracted}/${bin}.exe"; do
            if [ -f "${candidate}" ]; then
                cp "${candidate}" "${INSTALL_DIR}/"
                chmod 755 "${INSTALL_DIR}/$(basename "${candidate}")"
                break
            fi
        done
    done
    ok "installed from release ${tag}"
}

install_from_source() {
    have cargo || die "cargo not found.
  Install Rust from https://rustup.rs, then re-run this script.
  Rust ${MSRV:-1.97} or newer is required."

    mkdir -p "${INSTALL_DIR}"

    # cargo install --root places binaries in <root>/bin, so it cannot target an
    # arbitrary --dir directly. Build into a scratch root, then copy across.
    WORKDIR=$(mktemp -d 2>/dev/null || mktemp -d -t agentlens)
    root="${WORKDIR}/cargo-root"

    if [ -f "${SCRIPT_DIR}/Cargo.toml" ] && [ -d "${SCRIPT_DIR}/crates" ]; then
        step "building from the local checkout (this takes a few minutes)"
        cargo install --locked --root "${root}" --force \
            --path "${SCRIPT_DIR}/crates/agentlens-cli"
        cargo install --locked --root "${root}" --force \
            --path "${SCRIPT_DIR}/crates/doclens-cli"
    else
        step "building from git (this takes a few minutes)"
        cargo install --locked --root "${root}" --force \
            --git "https://github.com/${REPO}" agentlens-cli
        cargo install --locked --root "${root}" --force \
            --git "https://github.com/${REPO}" doclens-cli
    fi

    for bin in agentlens doclens; do
        for candidate in "${root}/bin/${bin}" "${root}/bin/${bin}.exe"; do
            if [ -f "${candidate}" ]; then
                cp "${candidate}" "${INSTALL_DIR}/"
                chmod 755 "${INSTALL_DIR}/$(basename "${candidate}")"
                break
            fi
        done
    done
    ok "built and installed from source"
}

# --------------------------------------------------------------- post-install --

verify_install() {
    missing=""
    for bin in agentlens doclens; do
        if [ ! -x "${INSTALL_DIR}/${bin}" ] && [ ! -x "${INSTALL_DIR}/${bin}.exe" ]; then
            missing="${missing} ${bin}"
        fi
    done
    [ -z "${missing}" ] || die "install finished but these binaries are missing:${missing}"

    if "${INSTALL_DIR}/agentlens" --version >/dev/null 2>&1; then
        ok "$("${INSTALL_DIR}/agentlens" --version 2>&1 | head -1)"
    else
        die "${INSTALL_DIR}/agentlens is present but will not run"
    fi
}

# Prints the export line rather than editing a shell rc. Rewriting someone's
# config without asking is not this script's business.
check_path() {
    case ":${PATH}:" in
        *":${INSTALL_DIR}:"*) return 0 ;;
    esac

    say ""
    warn "${INSTALL_DIR} is not on your PATH"
    say "  Add this to your shell profile:"
    say ""
    say "      export PATH=\"${INSTALL_DIR}:\$PATH\""
    say ""
}

offer_agent_kit() {
    [ "${WANT_AGENT_KIT}" = true ] || return 0

    kit="${SCRIPT_DIR}/agent-kit/install-kit.sh"
    if [ ! -f "${kit}" ]; then
        say ""
        say "To install the agentlens skill and scout agent for Claude Code or"
        say "GitHub Copilot, clone the repo and run agent-kit/install-kit.sh:"
        say ""
        say "      git clone https://github.com/${REPO}"
        say "      sh agentlens/agent-kit/install-kit.sh"
        say ""
        return 0
    fi

    say ""
    say "The agent kit adds an 'agentlens' skill and an 'agentlens-scout'"
    say "subagent, so a coding agent knows how to drive these binaries."

    if [ "${ASSUME_YES}" != true ]; then
        if [ ! -t 0 ]; then
            say "Run 'sh agent-kit/install-kit.sh' to install it."
            return 0
        fi
        printf 'Install the agent kit now? [y/N] '
        read -r reply || reply=""
        case "${reply}" in
            y | Y | yes | YES) ;;
            *)
                say "Skipped. Run 'sh agent-kit/install-kit.sh' whenever you like."
                return 0
                ;;
        esac
    fi

    sh "${kit}" --claude
}

# ---------------------------------------------------------------------- main --

main() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --version)
                [ $# -ge 2 ] || die "--version needs a tag"
                VERSION=$2
                shift 2
                ;;
            --dir)
                [ $# -ge 2 ] || die "--dir needs a path"
                INSTALL_DIR=$2
                shift 2
                ;;
            --from-source)
                FROM_SOURCE=true
                shift
                ;;
            --yes | -y)
                ASSUME_YES=true
                shift
                ;;
            --no-agent-kit)
                WANT_AGENT_KIT=false
                shift
                ;;
            --help | -h)
                usage
                exit 0
                ;;
            *) die "unknown argument: $1 (try --help)" ;;
        esac
    done

    SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)

    step "install directory: ${INSTALL_DIR}"

    if [ "${FROM_SOURCE}" = true ]; then
        install_from_source
    else
        target=$(detect_target)
        if [ -z "${target}" ]; then
            warn "no prebuilt binary for $(uname -s) $(uname -m); building from source"
            install_from_source
        else
            step "detected platform: ${target}"
            [ -n "${VERSION}" ] || VERSION=$(resolve_version)

            if [ -z "${VERSION}" ]; then
                warn "no published release found; building from source"
                install_from_source
            elif ! install_from_release "${target}" "${VERSION}"; then
                warn "falling back to a source build"
                install_from_source
            fi
        fi
    fi

    verify_install
    check_path
    offer_agent_kit
}

main "$@"
