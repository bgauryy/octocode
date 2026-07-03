#!/usr/bin/env bash
# octocode-awareness PreToolUse(Write|Edit) hook.
# Claims the target file for this agent BEFORE it is modified so concurrent
# agents see the lock. Blocks the edit (exit 2) only when another agent already
# holds the file; fails open (exit 0) on any other error.
#
# awareness.mjs and extract-hook-files.mjs are injected into this scripts/ dir
# at build time from packages/octocode-memory/dist/bin/ — see skills/scripts/sync.mjs.
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

out="$(node "$ROOT/awareness.mjs" pre-flight-intent \
  --agent-id "${agent:-claude-agent}" \
  --rationale "auto: file edit via lifecycle hook" \
  "${target_args[@]}" \
  --test-plan "post-edit verification" \
  --ttl-minutes 15 2>&1)"
code=$?

if [ "$code" -eq 2 ]; then
  echo "octocode-awareness: target file is locked by another agent — edit blocked." >&2
  echo "$out" >&2
  exit 2
elif [ "$code" -ne 0 ]; then
  echo "octocode-awareness pre-flight warning (continuing): $out" >&2
fi
exit 0
