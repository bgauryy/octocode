#!/usr/bin/env bash
# record.sh — Write q{N}/output.md. Metrics come from $LOG (set by init-run.sh).
# Usage: bash record.sh <q_num> <model> <answer_file>
# Env:   RUN, LOG  (from init-run.sh)
set -euo pipefail
: "${RUN:?RUN required}"; : "${LOG:?LOG required}"
Q="${1:?q_num required}"
MODEL="${2:?model required}"
ANS="${3:?answer_file required}"
[[ -f "$ANS" ]] || { echo "answer file not found: $ANS" >&2; exit 1; }

D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
read CALLS IN OUT EL < <(node "$D/aggregate.mjs" "$LOG" "$Q")
TITLE=$(grep -m1 "^### Q${Q} —" "$D/../QUESTIONS.md" 2>/dev/null | sed 's/^### //' || echo "Question $Q")
OUT_PATH="$RUN/q${Q}/output.md"

{
  echo "# $TITLE"
  echo
  echo "## Metadata"
  echo
  echo "| Field       | Value |"
  echo "|-------------|-------|"
  echo "| Model       | $MODEL |"
  echo "| Calls       | $CALLS |"
  echo "| In Chars    | $IN |"
  echo "| Out Chars   | $OUT |"
  echo "| Elapsed ms  | $EL |"
  echo
  echo "## Answer"
  echo
  cat "$ANS"
} > "$OUT_PATH"

echo "[Q$Q] calls=$CALLS in=$IN out=$OUT ms=$EL → $OUT_PATH"
