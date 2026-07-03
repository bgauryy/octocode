#!/usr/bin/env bash
# octocode-awareness UserPromptSubmit hook — deliver unread notifications.
# Injects unread repo messages into context via additionalContext.
# Non-blocking, fail-open. Opt out with OCTOCODE_NO_NOTIFY=1.
#
# awareness.mjs is injected at build time.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

[ "${OCTOCODE_NO_NOTIFY:-0}" = "1" ] && exit 0

agent="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{const o=JSON.parse(d);process.stdout.write((o.session_id||"claude-agent")+"\n")}catch{process.stdout.write("claude-agent\n")}})' 2>/dev/null)"
agent="${OCTOCODE_AGENT_ID:-${agent:-claude-agent}}"
cwd="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{process.stdout.write((JSON.parse(d).cwd||"")+"\n")}catch{process.stdout.write("\n")}})' 2>/dev/null)"

node "$ROOT/awareness.mjs" notify-get \
  --agent-id "$agent" \
  ${cwd:+--workspace "$cwd"} \
  --unread-only --mark-read --format hook 2>/dev/null || exit 0
exit 0
