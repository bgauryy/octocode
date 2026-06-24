#!/usr/bin/env bash
# octocode-awareness PreToolUse(Write|Edit) hook — harness self-fix gate.
# An agent MAY fix the skill itself, but only under human control: this hook
# BLOCKS edits to files inside the skill's own directory unless a human opened
# the gate (OCTOCODE_ALLOW_HARNESS_APPLY=1) AND the skill's repo is on a dedicated
# branch (not main/master). Edits to any file OUTSIDE the skill are a no-op here
# (the normal pre-edit lock hook handles those). Supports Claude-style file_path
# inputs and Codex apply_patch command payloads, mirroring the lock hooks.
set -uo pipefail

SKILL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
input="$(cat)"

files="$(printf '%s' "$input" | python3 -c '
import json
import re
import sys

try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)

tool_input = data.get("tool_input") or {}
paths = []

def add(value):
    if isinstance(value, str) and value.strip():
        paths.append(value.strip())
    elif isinstance(value, list):
        for item in value:
            add(item)

add(tool_input.get("file_path"))
add(tool_input.get("path"))
add(tool_input.get("file_paths"))

command = tool_input.get("command")
if isinstance(command, str):
    for line in command.splitlines():
        match = re.match(r"^\*\*\* (?:Add|Update|Delete) File: (.+)$", line)
        if match:
            paths.append(match.group(1).strip())
            continue
        match = re.match(r"^\*\*\* Move to: (.+)$", line)
        if match:
            paths.append(match.group(1).strip())

seen = set()
for path in paths:
    if path and path not in seen:
        seen.add(path)
        print(path)
' 2>/dev/null)"
[ -z "${files:-}" ] && exit 0

# Only act on edits to the skill's OWN files.
inside_skill=0
while IFS= read -r file; do
  [ -z "$file" ] && continue
  case "$file" in /*) abs="$file" ;; *) abs="$PWD/$file" ;; esac
  case "$abs" in
    "$SKILL_ROOT"/*) inside_skill=1 ;;
  esac
done <<EOF
$files
EOF

[ "$inside_skill" -eq 1 ] || exit 0

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
