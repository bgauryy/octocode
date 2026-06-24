#!/usr/bin/env bash
# octocode-awareness SessionEnd hook (2.1 auto-capture).
# At session end, write a work-handoff refinement from this session's file locks
# + the dirty git tree, so the next agent can pick up. Non-blocking, fail-open;
# session-capture itself no-ops on a clean tree with no session locks.
# Opt out with OCTOCODE_NO_SESSION_CAPTURE=1; skipped on a `clear`.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

[ "${OCTOCODE_NO_SESSION_CAPTURE:-0}" = "1" ] && exit 0

reason="$(printf '%s' "$input" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("reason") or "")' 2>/dev/null)"
[ "$reason" = "clear" ] && exit 0

agent="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id") or "claude-agent")' 2>/dev/null)"
agent="${OCTOCODE_AGENT_ID:-${agent:-claude-agent}}"

python3 "$ROOT/awareness.py" session-capture --agent-id "$agent" >/dev/null 2>&1 || true
exit 0
