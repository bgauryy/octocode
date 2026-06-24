#!/usr/bin/env bash
# octocode-awareness UserPromptSubmit hook — repo-scoped agent messaging delivery.
# On every turn, injects messages other agents posted to THIS repo (addressed to
# me or broadcast, still unread) into my context via `additionalContext`, then
# advances my read cursor so each message is delivered once. Non-blocking and
# fail-open: any error exits 0 with empty output so a hook bug never wedges work.
# Opt out with OCTOCODE_NO_NOTIFY=1.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

[ "${OCTOCODE_NO_NOTIFY:-0}" = "1" ] && exit 0

# Shared identity: OCTOCODE_AGENT_ID lets the hook and manual notify calls act as
# the same agent. Falls back to the session id (so concurrent sessions differ).
agent="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id") or "claude-agent")' 2>/dev/null)"
agent="${OCTOCODE_AGENT_ID:-${agent:-claude-agent}}"

# Read the right repo channel from the prompt's cwd (the workspace DB lives there).
cwd="$(printf '%s' "$input" | python3 -c 'import sys,json;print(json.load(sys.stdin).get("cwd",""))' 2>/dev/null)"

# --format hook prints the UserPromptSubmit additionalContext payload when there
# are unread messages, and nothing at all when the inbox is clear (a true no-op).
python3 "$ROOT/awareness.py" notify-get \
  --agent-id "$agent" \
  ${cwd:+--workspace "$cwd"} \
  --unread-only --mark-read --format hook 2>/dev/null || exit 0
exit 0
