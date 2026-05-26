#!/usr/bin/env bash
# octo-meas.sh — Manual fallback metering for Octocode calls when the
# mcp-meas.mjs stdio proxy can't be used (e.g. the agent's MCP client cannot
# be reconfigured). Agent invokes once per tool call after the response is in
# context.
#
# Usage:
#   octo-meas.sh <tool> <request-file> <response-file> [elapsed_ms]
#
# Request/response files must contain ONLY the payload, not the JSON-RPC
# envelope — same ruler as mcp-meas.mjs:
#   request  = stringified params.arguments
#   response = concatenated result.content[].text
#
# Env: LOG, RUN required.
# Current Q is read from $RUN/.current-q sentinel.
set -euo pipefail
: "${LOG:?LOG required}"; : "${RUN:?RUN required}"
D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

SENTINEL="$RUN/.current-q"
[[ -f "$SENTINEL" ]] || { echo "octo-meas: $SENTINEL missing — run scripts/set-q.sh <n> first" >&2; exit 2; }
Q=$(cat "$SENTINEL" | tr -d '[:space:]')
[[ "$Q" =~ ^[0-9]+$ ]] || { echo "octo-meas: invalid Q in sentinel: $Q" >&2; exit 2; }

TOOL="${1:?tool required}"
REQ="${2:?request file required}"
RES="${3:?response file required}"
EL="${4:-0}"

[[ -f "$REQ" ]] || { echo "octo-meas: request file not found: $REQ" >&2; exit 1; }
[[ -f "$RES" ]] || { echo "octo-meas: response file not found: $RES" >&2; exit 1; }

IN=$(node "$D/chars.mjs" --file "$REQ")
OUT=$(node "$D/chars.mjs" --file "$RES")
node -e '
  const [q,tool,in_,out,el] = process.argv.slice(1);
  process.stdout.write(JSON.stringify({
    ts: new Date().toISOString(), q: +q, agent: "octocode", cmd: tool,
    in_chars: +in_, out_chars: +out, elapsed_ms: +el, exit: 0
  }) + "\n");
' "$Q" "$TOOL" "$IN" "$OUT" "$EL" >> "$LOG"
