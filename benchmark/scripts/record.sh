#!/usr/bin/env bash
# record.sh — Write q<N>.md (human) + q<N>.json (machine).
#
# Usage: bash record.sh <q_num> <model> <answer_file>
# Env:   RUN, LOG, QUESTIONS_FILE  (from init-run.sh)
#
# Behaviour:
#   - Verifies the question sentinel matches <q_num>.
#   - Aggregates metrics for <q_num> from $LOG via aggregate.mjs --json.
#   - Computes q_elapsed_ms (now − $RUN/.q-start) = total wall time the AGENT
#     spent on this Q including reasoning between calls.
#   - Writes q<N>.json (canonical machine source of truth).
#   - Writes q<N>.md (human view).
#   - Fails loud if no calls were recorded (unless --allow-zero).
set -euo pipefail
: "${RUN:?RUN required}"; : "${LOG:?LOG required}"

ALLOW_ZERO=""
if [[ "${1:-}" == "--allow-zero" ]]; then ALLOW_ZERO="--allow-zero"; shift; fi

Q="${1:?q_num required}"
MODEL="${2:?model required}"
ANS="${3:?answer_file required}"
[[ -f "$ANS" ]] || { echo "answer file not found: $ANS" >&2; exit 1; }
[[ "$Q" =~ ^[0-9]+$ ]] || { echo "q_num must be integer" >&2; exit 1; }

D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"

# Questions file — exported by init-run.sh; fall back to the default question bank.
QFILE="${QUESTIONS_FILE:-$(cd "$D/.." && pwd)/questions/nextjs.md}"
[[ -f "$QFILE" ]] || { echo "record: questions file not found: $QFILE" >&2; exit 1; }

# Sentinel sanity check.
SENTINEL="$RUN/.current-q"
if [[ -f "$SENTINEL" ]]; then
  CUR=$(cat "$SENTINEL" | tr -d '[:space:]')
  if [[ "$CUR" == "0" ]]; then
    echo "record: current question is 0. Run benchmark/scripts/set-q.sh $Q before recording. (use --allow-zero to bypass)" >&2
    [[ -n "$ALLOW_ZERO" ]] || exit 2
  elif [[ "$CUR" != "$Q" ]]; then
    echo "record: sentinel says Q=$CUR but you're recording Q=$Q. Did you forget set-q.sh? (use --allow-zero to bypass)" >&2
    [[ -n "$ALLOW_ZERO" ]] || exit 2
  fi
fi

METRICS_JSON="$RUN/q${Q}.json"
OUT_PATH="$RUN/q${Q}.md"

# Q-level wall clock.
QSTART_FILE="$RUN/.q-start"
NOW_MS=$(perl -MTime::HiRes=time -e 'printf "%d\n", time()*1000' 2>/dev/null || \
          node -e 'process.stdout.write(String(Date.now()))')
if [[ -f "$QSTART_FILE" ]]; then
  START_MS=$(cat "$QSTART_FILE" | tr -d '[:space:]')
  if [[ "$START_MS" =~ ^[0-9]+$ ]]; then
    Q_ELAPSED_MS=$((NOW_MS - START_MS))
  else
    Q_ELAPSED_MS=0
  fi
else
  Q_ELAPSED_MS=0
fi

if [[ "${DETERMINISTIC:-0}" == "1" ]]; then Q_ELAPSED_MS=0; fi

TMP_METRICS=$(mktemp)
trap 'rm -f "$TMP_METRICS"' EXIT
node "$D/aggregate.mjs" "$LOG" "$Q" --json $ALLOW_ZERO > "$TMP_METRICS"

node -e '
  const fs = require("fs");
  const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  m.tool_elapsed_ms = m.elapsed_ms;
  m.q_elapsed_ms = +process.argv[2];
  fs.writeFileSync(process.argv[3], JSON.stringify(m));
' "$TMP_METRICS" "$Q_ELAPSED_MS" "$METRICS_JSON"

read CALLS IN OUT EL < <(node "$D/aggregate.mjs" "$LOG" "$Q" $ALLOW_ZERO)

QBLOCK=$(awk -v q="$Q" '
  $0 ~ "^### Q"q" —" { p=1; print; next }
  p && /^### Q[0-9]+ —/ { exit }
  p { print }
' "$QFILE")

AGENT=$(basename "$RUN")
if [[ "${DETERMINISTIC:-0}" == "1" ]]; then TS="—"; else TS=$(date -u +%Y-%m-%dT%H:%M:%SZ); fi

{
  printf '%s\n\n' "$QBLOCK"
  echo "## Metadata"
  echo
  echo "| Field           | Value |"
  echo "|-----------------|-------|"
  echo "| Run             | $(basename "$RUN") |"
  echo "| Agent           | $AGENT |"
  echo "| Model           | $MODEL |"
  echo "| Recorded        | $TS |"
  echo "| Calls           | $CALLS |"
  echo "| In Chars        | $IN |"
  echo "| Out Chars       | $OUT |"
  echo "| Tool elapsed ms | $EL |"
  echo "| Q elapsed ms    | $Q_ELAPSED_MS |"
  echo
  echo "## Answer"
  echo
  awk 'NR==1 && /^##[[:space:]]*Answer[[:space:]]*$/ { skip=1; next }
       NR==2 && skip && /^[[:space:]]*$/ { skip=0; next }
       { skip=0; print }' "$ANS"
} > "$OUT_PATH"

REASON_MS=$((Q_ELAPSED_MS - EL))
(( REASON_MS < 0 )) && REASON_MS=0
echo "[Q$Q] calls=$CALLS in_chars=$IN out_chars=$OUT tool_ms=$EL q_ms=$Q_ELAPSED_MS reason_ms≈$REASON_MS → $OUT_PATH"
