#!/usr/bin/env bash
# octocode-awareness Stop / SubagentStop hook — validate before conclude.
# Runs audit-unverified; blocks (exit 2) when unverified work exists.
# Loop-guarded via stop_hook_active. Opt out with OCTOCODE_NO_VERIFY_GATE=1.
#
# awareness.mjs is injected at build time.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

[ "${OCTOCODE_NO_VERIFY_GATE:-0}" = "1" ] && exit 0

looping="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{process.stdout.write(JSON.parse(d).stop_hook_active?"1\n":"0\n")}catch{process.stdout.write("0\n")}})' 2>/dev/null)"
[ "${looping:-0}" = "1" ] && exit 0

agent="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{const o=JSON.parse(d);process.stdout.write((o.session_id||"claude-agent")+"\n")}catch{process.stdout.write("claude-agent\n")}})' 2>/dev/null)"
agent="${OCTOCODE_AGENT_ID:-${agent:-claude-agent}}"
workspace="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{process.stdout.write((JSON.parse(d).cwd||"")+"\n")}catch{process.stdout.write("\n")}})' 2>/dev/null)"
workspace_args=()
[ -n "${workspace:-}" ] && workspace_args+=(--workspace "$workspace")

report="$(node "$ROOT/awareness.mjs" audit-unverified --agent-id "$agent" "${workspace_args[@]}" 2>/dev/null)"
status=$?
if [ "$status" -eq 1 ]; then
  plans="$(printf '%s' "$report" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{const o=JSON.parse(d);process.stdout.write((o.unverified||[]).map(u=>u.status+":"+u.intent_id+": "+u.test_plan).join("; ")+"\n")}catch{process.stdout.write("\n")}})' 2>/dev/null)"
  echo "octocode-awareness: concluding with unverified work. Pending: ${plans}" >&2
  exit 2
fi
exit 0
