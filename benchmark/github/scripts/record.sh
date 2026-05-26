#!/usr/bin/env bash
# record.sh — Write q<N>.md (human) + q<N>.json (machine).
#
# Usage: bash record.sh <q_num> <model> <answer_file>
# Env:   RUN, LOG  (from init-run.sh)
#
# Behaviour:
#   - Verifies the question sentinel matches <q_num> (catches "wrong Q" errors).
#   - Aggregates metrics for <q_num> from $LOG via aggregate.mjs --json.
#   - Computes q_elapsed_ms (now − $RUN/.q-start) = total wall time the AGENT
#     spent on this Q including reasoning between calls. Distinct from
#     tool_elapsed_ms (sum of tool-call wall times, from the log).
#   - Writes q<N>.json (canonical machine source of truth).
#   - Writes output.md (human view) referencing the same numbers.
#   - Fails loud if no calls were recorded for this question (unless --allow-zero).
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

# Sentinel sanity check.
SENTINEL="$RUN/.current-q"
if [[ -f "$SENTINEL" ]]; then
  CUR=$(cat "$SENTINEL" | tr -d '[:space:]')
  if [[ "$CUR" != "$Q" && "$CUR" != "0" ]]; then
    echo "record: sentinel says Q=$CUR but you're recording Q=$Q. Did you forget set-q.sh? (use --allow-zero to bypass)" >&2
    [[ -n "$ALLOW_ZERO" ]] || exit 2
  fi
fi

# Flat layout: q1.json + q1.md live directly in $RUN (no per-Q subdirs)
METRICS_JSON="$RUN/q${Q}.json"
OUT_PATH="$RUN/q${Q}.md"

# Q-level wall clock: from set-q.sh's .q-start to now.
# Captures the AGENT'S TOTAL Q TIME (reasoning + tool waits + answer writing),
# unlike the log-derived elapsed_ms which only sums individual tool-call waits.
# Use perl for millisecond timestamp — avoids a node spawn on the hot path so
# the end-timestamp overhead matches set-q.sh's start-timestamp overhead.
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

# DETERMINISTIC=1 zeroes the Q wall-clock too (wall clock is non-det).
if [[ "${DETERMINISTIC:-0}" == "1" ]]; then Q_ELAPSED_MS=0; fi

# Canonical metrics from the log (tool-call times) — write to a temp first,
# then augment with q_elapsed_ms and tool_elapsed_ms aliasing.
TMP_METRICS=$(mktemp)
trap 'rm -f "$TMP_METRICS"' EXIT
node "$D/aggregate.mjs" "$LOG" "$Q" --json $ALLOW_ZERO > "$TMP_METRICS"

# Augment with q_elapsed_ms and an explicit tool_elapsed_ms alias for clarity.
node -e '
  const fs = require("fs");
  const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  m.tool_elapsed_ms = m.elapsed_ms;   // alias: sum of tool-call wall times
  m.q_elapsed_ms = +process.argv[2];  // total Q wall time (set-q.sh → record.sh)
  fs.writeFileSync(process.argv[3], JSON.stringify(m));
' "$TMP_METRICS" "$Q_ELAPSED_MS" "$METRICS_JSON"

# Convenience scalars for the markdown view.
read CALLS IN OUT EL < <(node "$D/aggregate.mjs" "$LOG" "$Q" $ALLOW_ZERO)

# Question block (title + body) from QUESTIONS.md so output is self-contained.
QBLOCK=$(awk -v q="$Q" '
  $0 ~ "^### Q"q" —" { p=1; print; next }
  p && /^### Q[0-9]+ —/ { exit }
  p { print }
' "$D/../QUESTIONS.md")

# With the fixed layout $RUN = output/<agent>, basename is just the agent slug.
AGENT=$(basename "$RUN")
# DETERMINISTIC=1 omits the wall-clock timestamp so output.md becomes
# byte-identical across reruns of the same log. Use for golden-file tests.
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
  # Strip a leading '## Answer' line (and optional blank line) from the answer
  # file — record.sh adds its own header, so including it creates a double-header
  # that breaks answer extraction.
  awk 'NR==1 && /^##[[:space:]]*Answer[[:space:]]*$/ { skip=1; next }
       NR==2 && skip && /^[[:space:]]*$/ { skip=0; next }
       { skip=0; print }' "$ANS"
} > "$OUT_PATH"

REASON_MS=$((Q_ELAPSED_MS - EL))
(( REASON_MS < 0 )) && REASON_MS=0
echo "[Q$Q] calls=$CALLS in_chars=$IN out_chars=$OUT tool_ms=$EL q_ms=$Q_ELAPSED_MS reason_ms≈$REASON_MS → $OUT_PATH"
