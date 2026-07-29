#!/usr/bin/env bash
# The only thing allowed to bump current_phase or stage the next snapshot.
#
# PHASE_LOCK is the authority: the isolation hook reads it, and nothing but this
# script writes it. LOOP_STATE.md is the agent's scratchpad and is deliberately
# NOT trusted here — an agent that can rewrite its own phase number can walk
# itself out of isolation.
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

LOCK=".agent-lens-phase/PHASE_LOCK"
STATE=".agent-lens-phase/LOOP_STATE.md"
VAULT="${HOME}/.agent-lens-vault"

die() {
    echo "advance-phase: $*" >&2
    exit 1
}

[[ -f ${LOCK} ]] || die "${LOCK} missing; cannot establish current phase."

n=$(grep -oP 'current_phase:\s*\K\d+' "${LOCK}") ||
    die "${LOCK} has no 'current_phase: N' line."

# The PR is found by head branch, recorded by the loop in LOOP_STATE.md. Searching
# for "agentlensphase$n" would never match, because branches are named feat/<slug>.
[[ -f ${STATE} ]] || die "${STATE} missing; cannot determine the branch for phase ${n}."
branch=$(grep -oP '^\s*current_branch:\s*\K\S+' "${STATE}") ||
    die "${STATE} has no 'current_branch: <name>' line for phase ${n}."

merged=$(gh pr list --head "${branch}" --state merged --json number --jq 'length')
[[ ${merged} -gt 0 ]] || die "phase ${n} (${branch}) not merged; refusing."

# Glob, not a strict name: snapshots may carry a descriptive suffix, e.g.
# agentlensphase3doctool.zip. A missing match is a hard error — the previous
# `|| true` swallowed it and bumped the counter anyway, silently skipping a phase.
next=$((n + 1))
shopt -s nullglob
candidates=("${VAULT}/agentlensphase${next}"*.zip)

if (( ${#candidates[@]} == 0 )); then
    remaining=("${VAULT}"/*.zip)
    shopt -u nullglob
    if (( ${#remaining[@]} == 0 )); then
        echo "NO_PHASES_REMAIN"
        exit 0
    fi
    die "no zip matching agentlensphase${next}*.zip, but ${#remaining[@]} zip(s) remain in the vault. Refusing to skip a phase."
fi
shopt -u nullglob

(( ${#candidates[@]} == 1 )) ||
    die "ambiguous: ${#candidates[@]} zips match agentlensphase${next}*.zip"

mv "${candidates[0]}" .agent-lens-phase/
sed -i "s/current_phase: ${n}/current_phase: ${next}/" "${LOCK}"

echo "advanced to phase ${next}: $(basename "${candidates[0]}")"
