#!/bin/sh
# Deploy the agentlens skill and scout agent to Claude Code and/or GitHub Copilot.
#
# The skill and agent are authored once in this directory. The two toolchains
# want different layouts and different frontmatter, so this script assembles
# each rather than the repo carrying two copies that drift.
#
# Usage:
#   sh install-kit.sh                      # Claude Code (global)
#   sh install-kit.sh --copilot            # Copilot, into the current repo
#   sh install-kit.sh --claude --copilot ~/src/myproject
#   sh install-kit.sh --dry-run

set -eu

WANT_CLAUDE=false
WANT_COPILOT=false
COPILOT_DIR=""
DRY_RUN=false
EXPLICIT=false

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_DIM=$(printf '\033[2m')
    C_RED=$(printf '\033[31m')
    C_GREEN=$(printf '\033[32m')
    C_OFF=$(printf '\033[0m')
else
    C_DIM='' C_RED='' C_GREEN='' C_OFF=''
fi

say() { printf '%s\n' "$*"; }
step() { printf '%s==>%s %s\n' "${C_DIM}" "${C_OFF}" "$*"; }
ok() { printf '%sok:%s %s\n' "${C_GREEN}" "${C_OFF}" "$*"; }
die() {
    printf '%serror:%s %s\n' "${C_RED}" "${C_OFF}" "$*" >&2
    exit 1
}

usage() {
    cat <<'EOF'
Deploy the agentlens skill and scout agent.

  --claude           Install for Claude Code, into ~/.claude (default when
                     no target is given).
  --copilot [dir]    Install for GitHub Copilot, into <dir>/.github.
                     Defaults to the current directory when it is a git repo.
  --dry-run          Print what would be written, write nothing.
  --help             Show this message.

Claude Code agents are global; Copilot agents are per-repository. That is why
the Copilot target takes a directory and the Claude one does not.
EOF
}

# Assembles frontmatter + body into a single agent definition. Kept in one place
# so the two toolchains cannot drift apart.
render_agent() {
    frontmatter=$1
    body=$2
    out=$3

    # The redirect must wrap the body, not the function definition: on a
    # definition it expands ${out} at call time, before the body assigns it.
    {
        printf -- '---\n'
        cat "${frontmatter}"
        printf -- '---\n\n'
        cat "${body}"
    } >"${out}"
}

copy_skill() {
    dest=$1
    mkdir -p "${dest}/references"
    cp "${SRC}/SKILL.md" "${dest}/SKILL.md"
    cp "${SRC}/references/commands.md" "${dest}/references/commands.md"
    cp "${SRC}/references/addressing.md" "${dest}/references/addressing.md"
    cp "${SRC}/references/recipes.md" "${dest}/references/recipes.md"
}

install_claude() {
    skill_dir="${HOME}/.claude/skills/agentlens"
    agent_file="${HOME}/.claude/agents/agentlens-scout.md"

    step "Claude Code"
    say "  skill -> ${skill_dir}/"
    say "  agent -> ${agent_file}"
    [ "${DRY_RUN}" = true ] && return 0

    copy_skill "${skill_dir}"
    mkdir -p "$(dirname "${agent_file}")"
    render_agent "${SRC}/frontmatter/agent.claude.yaml" "${SRC}/agent.md" "${agent_file}"
    ok "installed for Claude Code"
}

install_copilot() {
    dir=$1
    skill_dir="${dir}/.github/skills/agentlens"
    agent_file="${dir}/.github/agents/agentlens-scout.md"

    step "GitHub Copilot"
    say "  skill -> ${skill_dir}/"
    say "  agent -> ${agent_file}"
    [ "${DRY_RUN}" = true ] && return 0

    copy_skill "${skill_dir}"
    mkdir -p "$(dirname "${agent_file}")"
    render_agent "${SRC}/frontmatter/agent.copilot.yaml" "${SRC}/agent.md" "${agent_file}"
    ok "installed for GitHub Copilot"
}

main() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --claude)
                WANT_CLAUDE=true
                EXPLICIT=true
                shift
                ;;
            --copilot)
                WANT_COPILOT=true
                EXPLICIT=true
                shift
                # An optional directory may follow, but not another flag.
                if [ $# -gt 0 ]; then
                    case "$1" in
                        -*) ;;
                        *)
                            COPILOT_DIR=$1
                            shift
                            ;;
                    esac
                fi
                ;;
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --help | -h)
                usage
                exit 0
                ;;
            *) die "unknown argument: $1 (try --help)" ;;
        esac
    done

    [ "${EXPLICIT}" = true ] || WANT_CLAUDE=true

    SRC=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
    [ -f "${SRC}/SKILL.md" ] || die "SKILL.md not found beside this script"
    [ -f "${SRC}/agent.md" ] || die "agent.md not found beside this script"

    [ "${DRY_RUN}" = true ] && say "dry run: nothing will be written"

    if [ "${WANT_CLAUDE}" = true ]; then
        install_claude
    fi

    if [ "${WANT_COPILOT}" = true ]; then
        if [ -z "${COPILOT_DIR}" ]; then
            if [ -d .git ]; then
                COPILOT_DIR=$(pwd)
            else
                die "--copilot needs a directory: the current directory is not a git repo.
  Copilot agents are per-repository, so there is no sensible global default."
            fi
        fi
        [ -d "${COPILOT_DIR}" ] || die "no such directory: ${COPILOT_DIR}"
        install_copilot "${COPILOT_DIR}"
    fi

    if [ "${DRY_RUN}" != true ]; then
        say ""
        say "The skill teaches an agent to drive agentlens and doclens."
        say "The agentlens-scout subagent answers a list of questions and"
        say "returns capped findings, so a primary agent spends little context."
    fi
}

main "$@"
