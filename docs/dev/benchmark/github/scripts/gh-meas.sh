#!/usr/bin/env bash
# gh-meas.sh — Run a gh command, log chars in/out + elapsed_ms to $LOG.
# Usage: bash gh-meas.sh <gh args...>
# Env:   LOG (jsonl path, required), Q (question number, required)
set -euo pipefail
: "${LOG:?LOG required}"; : "${Q:?Q required}"
D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

CMD="gh $*"
START=$(node -e 'process.stdout.write(String(Date.now()))')
TMP=$(mktemp); trap 'rm -f "$TMP"' EXIT
set +e; gh "$@" >"$TMP" 2>&1; EXIT=$?; set -e
END=$(node -e 'process.stdout.write(String(Date.now()))')

IN=$(printf '%s' "$CMD" | node "$D/chars.mjs")
OUT=$(node "$D/chars.mjs" --file "$TMP")
node -e '
  const [q,cmd,in_,out,el,ex] = process.argv.slice(1);
  process.stdout.write(JSON.stringify({
    ts: new Date().toISOString(), q: +q, agent: "gh", cmd,
    in_chars: +in_, out_chars: +out, elapsed_ms: +el, exit: +ex
  }) + "\n");
' "$Q" "$CMD" "$IN" "$OUT" $((END - START)) "$EXIT" >> "$LOG"

cat "$TMP"
exit $EXIT
