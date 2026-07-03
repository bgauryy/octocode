#!/usr/bin/env bash
# octocode-awareness SessionEnd hook — capture session state as handoff refinement.
# Non-blocking, fail-open. Opt out with OCTOCODE_NO_SESSION_CAPTURE=1.
# Skipped on reason=clear.
#
# awareness.mjs is injected at build time.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

[ "${OCTOCODE_NO_SESSION_CAPTURE:-0}" = "1" ] && exit 0

reason="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{process.stdout.write((JSON.parse(d).reason||"")+"\n")}catch{process.stdout.write("\n")}})' 2>/dev/null)"
[ "$reason" = "clear" ] && exit 0

agent="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{const o=JSON.parse(d);process.stdout.write((o.session_id||"claude-agent")+"\n")}catch{process.stdout.write("claude-agent\n")}})' 2>/dev/null)"
agent="${OCTOCODE_AGENT_ID:-${agent:-claude-agent}}"

node "$ROOT/awareness.mjs" session-capture --agent-id "$agent" >/dev/null 2>&1 || true
exit 0
