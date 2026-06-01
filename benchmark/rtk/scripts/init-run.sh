#!/usr/bin/env bash
# init-run.sh — Create one fixed benchmark run directory under output/.
#
# Usage: source init-run.sh <agent-slug>
#   <agent-slug> is any [a-z0-9_-]+ identifier. Conventional values:
#   `octocode`, `gh`, `none` — but any label works (e.g. `octocode-haiku`).
#
# Output semantics:
#   Runs are written directly under the benchmark output directory:
#     output/octocode/   ← RUN for octocode agent
#     output/gh/         ← RUN for gh agent
#
# Typical operator flow:
#   source scripts/init-run.sh octocode   # creates output/octocode/
#   source scripts/init-run.sh gh         # creates output/gh/
#
# Exports: $SESSION (output dir), $RUN (this agent's dir), $LOG (jsonl path)
#
# Layout created inside $RUN:
#   log.jsonl          ← single per-call log (mcp-meas.mjs + gh-meas.mjs)
#   .current-q         ← question sentinel; updated by set-q.sh
#   .q-count           ← total Q count (auto-derived from QUESTIONS.md)
#   (no per-Q subdirs — record.sh writes q1.md + q1.json flat in $RUN)
set -euo pipefail
A="${1:-}"
[[ "$A" =~ ^[a-z0-9_-]+$ ]] || {
  echo "Usage: source init-run.sh <agent-slug>   (lowercase alnum + _ + -)" >&2; return 1 2>/dev/null || exit 1
}
D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
QUESTIONS="$D/../QUESTIONS.md"
[[ -f "$QUESTIONS" ]] || { echo "init-run: $QUESTIONS not found" >&2; return 1 2>/dev/null || exit 1; }
N_QS=$(grep -cE '^### Q[0-9]+ —' "$QUESTIONS")
(( N_QS > 0 )) || { echo "init-run: no '### Q<n> —' headings found in QUESTIONS.md" >&2; return 1 2>/dev/null || exit 1; }

OUTPUT_DIR="$(cd "$D/.." && pwd)/output"
mkdir -p "$OUTPUT_DIR"
SESSION="$OUTPUT_DIR"
export SESSION

RUN="${OUTPUT_DIR}/${A}"
[[ ! -d "$RUN" ]] || { echo "init-run: $RUN already exists — remove it before starting a fresh run" >&2; return 1 2>/dev/null || exit 1; }
mkdir -p "$RUN"

LOG="$RUN/log.jsonl"
: > "$LOG"
printf '0\n'       > "$RUN/.current-q"
printf '%s\n' "$N_QS" > "$RUN/.q-count"
rm -f "$RUN/.q-start"

export RUN LOG
export Q=0
echo "$RUN"
