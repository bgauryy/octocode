#!/usr/bin/env bash
# octo-meas.sh — Log one Octocode MCP tool call to $LOG.
# Agent calls this after each MCP response is in context.
#
# Usage:
#   octo-meas.sh <tool> <request-file> <response-file> [elapsed_ms]
# Env:
#   LOG, Q  (required)
#
# Files are char-counted by chars.mjs (codepoints). The MCP server is not
# instrumented, so the agent is responsible for invoking this once per call.
set -euo pipefail
: "${LOG:?LOG required}"; : "${Q:?Q required}"
D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

TOOL="${1:?tool required}"
REQ="${2:?request file required}"
RES="${3:?response file required}"
EL="${4:-0}"

IN=$(node "$D/chars.mjs" --file "$REQ")
OUT=$(node "$D/chars.mjs" --file "$RES")
node -e '
  const [q,tool,in_,out,el] = process.argv.slice(1);
  process.stdout.write(JSON.stringify({
    ts: new Date().toISOString(), q: +q, agent: "octocode", cmd: tool,
    in_chars: +in_, out_chars: +out, elapsed_ms: +el, exit: 0
  }) + "\n");
' "$Q" "$TOOL" "$IN" "$OUT" "$EL" >> "$LOG"
