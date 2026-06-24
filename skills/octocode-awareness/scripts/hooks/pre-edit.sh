#!/usr/bin/env bash
# octocode-awareness PreToolUse(Write|Edit) hook.
# Claims the target file for this agent BEFORE it is modified, so concurrent
# agents see the lock. Blocks the edit (exit 2) only when another agent already
# holds the file; fails open (exit 0) on any other error so a hook bug never
# wedges real work.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

file="$(printf '%s' "$input" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("tool_input",{}).get("file_path",""))' 2>/dev/null)"
agent="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id") or "claude-agent")' 2>/dev/null)"
# Shared identity: OCTOCODE_AGENT_ID lets hooks and manual pre-flight-intent calls
# act as the same agent. Falls back to the session id.
agent="${OCTOCODE_AGENT_ID:-$agent}"
[ -z "${file:-}" ] && exit 0

out="$(python3 "$ROOT/awareness.py" pre-flight-intent \
  --agent-id "${agent:-claude-agent}" \
  --rationale "auto: file edit via Claude hook" \
  --target-file "$file" \
  --test-plan "post-edit verification" \
  --ttl-minutes 15 2>&1)"
code=$?

if [ "$code" -eq 2 ]; then
  echo "octocode-awareness: '$file' is locked by another agent — edit blocked." >&2
  echo "$out" >&2
  exit 2
elif [ "$code" -ne 0 ]; then
  echo "octocode-awareness pre-flight warning (continuing): $out" >&2
  exit 0
fi
exit 0
