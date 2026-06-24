#!/usr/bin/env bash
# octocode-awareness PostToolUse(Write|Edit) hook.
# Releases this agent's lock on the file it just modified. Non-blocking: always
# exits 0 (the edit already happened); lock TTL is the safety net if it fails.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

file="$(printf '%s' "$input" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))' 2>/dev/null)"
agent="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id") or "claude-agent")' 2>/dev/null)"
# Must match the identity pre-edit.sh claimed under.
agent="${OCTOCODE_AGENT_ID:-$agent}"
[ -z "${file:-}" ] && exit 0

python3 "$ROOT/awareness.py" release-file-lock \
  --agent-id "${agent:-claude-agent}" \
  --target-file "$file" \
  --status SUCCESS >/dev/null 2>&1 || true
exit 0
