#!/usr/bin/env bash
# octocode-awareness Stop / SubagentStop hook (1.1 validate-before-conclude).
# Warns once when the agent concludes with an intent that declared a --test-plan
# but never recorded a VERIFIED event. This includes post-edit PENDING intents
# whose file locks were already released. Loop-guarded via stop_hook_active;
# opt-out with OCTOCODE_NO_VERIFY_GATE=1.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

[ "${OCTOCODE_NO_VERIFY_GATE:-0}" = "1" ] && exit 0

# Avoid re-blocking after we already prompted once this turn.
looping="$(printf '%s' "$input" | python3 -c 'import sys,json;print("1" if json.load(sys.stdin).get("stop_hook_active") else "0")' 2>/dev/null)"
[ "${looping:-0}" = "1" ] && exit 0

agent="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("session_id") or "claude-agent")' 2>/dev/null)"
agent="${OCTOCODE_AGENT_ID:-${agent:-claude-agent}}"
workspace="$(printf '%s' "$input" | python3 -c 'import sys,json;d=json.load(sys.stdin);print(d.get("cwd") or "")' 2>/dev/null)"
workspace_args=()
[ -n "${workspace:-}" ] && workspace_args+=(--workspace "$workspace")

report="$(python3 "$ROOT/awareness.py" audit-unverified --agent-id "$agent" "${workspace_args[@]}" 2>/dev/null)"
status=$?
if [ "$status" -eq 1 ]; then
  plans="$(printf '%s' "$report" | python3 -c 'import sys,json;d=json.load(sys.stdin);print("; ".join(u["status"]+":"+u["intent_id"]+": "+u["test_plan"] for u in d.get("unverified",[])))' 2>/dev/null)"
  echo "octocode-awareness: you are concluding with unverified work. Run the test_plan and record it (awareness.py verify --agent-id \"$agent\" ${workspace_args[*]} --all-pending --message \"<actual check result>\", or verify --intent-id <id>) before claiming success. Pending: ${plans}" >&2
  exit 2
fi
exit 0
