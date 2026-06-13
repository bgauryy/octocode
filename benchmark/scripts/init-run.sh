#!/usr/bin/env bash
# init-run.sh - Create one fixed benchmark run directory under output/.
#
# Usage: source init-run.sh <agent-slug>
#   <agent-slug> - conventional values: octocode, rtk-gh
#
# Output:
#   output/<agent-slug>/   ← RUN
#
# Exports:
#   $SESSION        - output dir
#   $RUN            - this agent's run dir
#   $LOG            - jsonl log path inside $RUN
#   $Q              - current question (starts at 0)
#   $QUESTIONS_FILE - absolute path to the active questions file
#
# Layout created inside $RUN:
#   log.jsonl     - per-call measurement log
#   .current-q    - question sentinel (updated by set-q.sh)
#   .q-count      - total questions (derived from the active questions file)
set -euo pipefail

A="${1:-}"
[[ "$A" =~ ^[a-z0-9_-]+$ ]] || {
  echo "Usage: source init-run.sh <agent-slug>   (e.g. octocode, rtk-gh)" >&2
  return 1 2>/dev/null || exit 1
}

# Validate OCTOCODE_CLI_BIN — must point to the local build, not a global install.
LOCAL_BUILD="/Users/guybary/Documents/octocode-mcp/packages/octocode-cli/out/octocode-cli.js"
if [[ -z "${OCTOCODE_CLI_BIN:-}" ]]; then
  echo "init-run: OCTOCODE_CLI_BIN not set — defaulting to local build: $LOCAL_BUILD" >&2
  export OCTOCODE_CLI_BIN="$LOCAL_BUILD"
fi
if [[ ! -f "$OCTOCODE_CLI_BIN" ]]; then
  echo "init-run: OCTOCODE_CLI_BIN does not exist: $OCTOCODE_CLI_BIN" >&2
  echo "  Run: cd /Users/guybary/Documents/octocode-mcp/packages/octocode-cli && yarn build" >&2
  return 1 2>/dev/null || exit 1
fi

D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
DEFAULT_QUESTIONS_FILE="$(cd "$D/.." && pwd)/questions/nextjs.md"
QUESTIONS_FILE="${QUESTIONS_FILE:-$DEFAULT_QUESTIONS_FILE}"

[[ -f "$QUESTIONS_FILE" ]] || {
  echo "init-run: questions file not found: $QUESTIONS_FILE" >&2
  return 1 2>/dev/null || exit 1
}

N_QS=$(grep -cE '^### Q[0-9]+ -' "$QUESTIONS_FILE")
(( N_QS > 0 )) || {
  echo "init-run: no '### Q<n> -' headings found in $QUESTIONS_FILE" >&2
  return 1 2>/dev/null || exit 1
}

OUTPUT_DIR="$(cd "$D/.." && pwd)/output"
mkdir -p "$OUTPUT_DIR"
SESSION="$OUTPUT_DIR"
export SESSION

RUN="${OUTPUT_DIR}/${A}"
[[ ! -d "$RUN" ]] || {
  echo "init-run: $RUN already exists - remove it before starting a fresh run" >&2
  return 1 2>/dev/null || exit 1
}
mkdir -p "$RUN"

LOG="$RUN/log.jsonl"
: > "$LOG"
printf '0\n'          > "$RUN/.current-q"
printf '%s\n' "$N_QS" > "$RUN/.q-count"
rm -f "$RUN/.q-start"

export RUN LOG Q=0 QUESTIONS_FILE
echo "$RUN"
