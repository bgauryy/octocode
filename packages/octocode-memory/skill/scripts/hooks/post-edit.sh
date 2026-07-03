#!/usr/bin/env bash
# octocode-awareness PostToolUse(Write|Edit) hook.
# Releases this agent's lock on the file it just modified (status=PENDING so
# the Stop hook can still require verification). Always exits 0.
#
# awareness.mjs and extract-hook-files.mjs are injected at build time.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
input="$(cat)"

files="$(printf '%s' "$input" | node "$ROOT/extract-hook-files.mjs" 2>/dev/null)"
agent="$(printf '%s' "$input" | node -e 'let d="";process.stdin.on("data",c=>d+=c);process.stdin.on("end",()=>{try{const o=JSON.parse(d);process.stdout.write((o.session_id||"claude-agent")+"\n")}catch{process.stdout.write("claude-agent\n")}})' 2>/dev/null)"
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

node "$ROOT/awareness.mjs" release-file-lock \
  --agent-id "${agent:-claude-agent}" \
  "${target_args[@]}" \
  --status PENDING >/dev/null 2>&1 || true
exit 0
