#!/usr/bin/env bash
# octocode-awareness PreToolUse(Write|Edit) hook.
# Claims the target file for this agent BEFORE it is modified, so concurrent
# agents see the lock. Blocks the edit (exit 2) only when another agent already
# holds the file; fails open (exit 0) on any other error so a hook bug never
# wedges real work. Supports Claude-style file_path inputs and Codex apply_patch
# command payloads.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
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
agent="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id") or "claude-agent")' 2>/dev/null)"
# Shared identity: OCTOCODE_AGENT_ID lets hooks and manual pre-flight-intent calls
# act as the same agent. Falls back to the session id.
agent="${OCTOCODE_AGENT_ID:-$agent}"
[ -z "${files:-}" ] && exit 0

target_args=()
while IFS= read -r file; do
  [ -z "$file" ] && continue
  target_args+=(--target-file "$file")
done <<EOF
$files
EOF

[ "${#target_args[@]}" -eq 0 ] && exit 0

out="$(python3 "$ROOT/awareness.py" pre-flight-intent \
  --agent-id "${agent:-claude-agent}" \
  --rationale "auto: file edit via lifecycle hook" \
  "${target_args[@]}" \
  --test-plan "post-edit verification" \
  --ttl-minutes 15 2>&1)"
code=$?

if [ "$code" -eq 2 ]; then
  echo "octocode-awareness: target file is locked by another agent — edit blocked." >&2
  echo "$out" >&2
  exit 2
elif [ "$code" -ne 0 ]; then
  echo "octocode-awareness pre-flight warning (continuing): $out" >&2
  exit 0
fi
exit 0
