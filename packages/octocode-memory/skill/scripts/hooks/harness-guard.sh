#!/usr/bin/env bash
# octocode-awareness PreToolUse harness self-fix gate.
# Blocks edits to the skill's own directory unless OCTOCODE_ALLOW_HARNESS_APPLY=1
# and the repo is on a dedicated branch.
#
# extract-hook-files.mjs is injected at build time.
set -uo pipefail

SKILL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

files="$(printf '%s' "$input" | node "$ROOT/extract-hook-files.mjs" 2>/dev/null)"
[ -z "${files:-}" ] && exit 0

inside_skill=0
while IFS= read -r file; do
  [ -z "$file" ] && continue
  case "$file" in /*) abs="$file" ;; *) abs="$PWD/$file" ;; esac
  case "$abs" in "$SKILL_ROOT"/*) inside_skill=1 ;; esac
done <<EOF
$files
EOF
[ "$inside_skill" -eq 1 ] || exit 0

if [ "${OCTOCODE_ALLOW_HARNESS_APPLY:-0}" != "1" ]; then
  echo "octocode-awareness: editing the skill itself is gated. A human must set OCTOCODE_ALLOW_HARNESS_APPLY=1. Edit blocked." >&2
  exit 2
fi

if [ "${OCTOCODE_HARNESS_BRANCH_OK:-0}" != "1" ]; then
  branch="$(git -C "$SKILL_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")"
  case "$branch" in
    ""|HEAD|main|master)
      echo "octocode-awareness: harness self-fix is branch-only (on '${branch:-detached}'). Create a dedicated branch first, or set OCTOCODE_HARNESS_BRANCH_OK=1. Edit blocked." >&2
      exit 2 ;;
  esac
fi
exit 0
