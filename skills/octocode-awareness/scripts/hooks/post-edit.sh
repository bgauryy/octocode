#!/usr/bin/env bash
# octocode-awareness PostToolUse(Write|Edit) hook.
# Releases this agent's lock on the file it just modified, but leaves the intent
# in PENDING so the Stop hook can still require verification before "done".
# Non-blocking: always exits 0 (the edit already happened). Waiters should use
# bounded polling (`wait-for-lock` / `pre-flight-intent --wait-seconds`); this
# hook and the TTL are the release signals that keep those waits from dangling.
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
# Must match the identity pre-edit.sh claimed under.
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

python3 "$ROOT/awareness.py" release-file-lock \
  --agent-id "${agent:-claude-agent}" \
  "${target_args[@]}" \
  --status PENDING >/dev/null 2>&1 || true
exit 0
