#!/usr/bin/env bash
# octocode-awareness PreToolUse(Write|Edit) hook — harness self-fix gate.
# An agent MAY fix the skill itself, but only under human control: this hook
# BLOCKS edits to files inside the skill's own directory unless a human opened
# the gate (OCTOCODE_ALLOW_HARNESS_APPLY=1) AND the skill's repo is on a dedicated
# branch (not main/master). Edits to any file OUTSIDE the skill are a no-op here
# (the normal pre-edit lock hook handles those). Mirrors how pre-edit.sh enforces
# file locks — the harness protects itself with the same mechanism.
set -uo pipefail

SKILL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
input="$(cat)"

file="$(printf '%s' "$input" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))' 2>/dev/null)"
[ -z "${file:-}" ] && exit 0
case "$file" in /*) abs="$file" ;; *) abs="$PWD/$file" ;; esac

# Only act on edits to the skill's OWN files.
case "$abs" in
  "$SKILL_ROOT"/*) : ;;
  *) exit 0 ;;
esac

# Gate 1 — explicit human approval for this session.
if [ "${OCTOCODE_ALLOW_HARNESS_APPLY:-0}" != "1" ]; then
  echo "octocode-awareness: editing the skill itself is gated. A human must approve by exporting OCTOCODE_ALLOW_HARNESS_APPLY=1 (and announce it via 'awareness.py harness-apply'). Edit blocked." >&2
  exit 2
fi

# Gate 2 — branch-only (reversible). Override with OCTOCODE_HARNESS_BRANCH_OK=1.
if [ "${OCTOCODE_HARNESS_BRANCH_OK:-0}" != "1" ]; then
  branch="$(git -C "$SKILL_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")"
  case "$branch" in
    ""|HEAD|main|master)
      echo "octocode-awareness: harness self-fix is branch-only (on '${branch:-detached}'). Create a dedicated branch (e.g. octocode-harness/<slug>) first, or set OCTOCODE_HARNESS_BRANCH_OK=1. Edit blocked." >&2
      exit 2 ;;
  esac
fi
exit 0
