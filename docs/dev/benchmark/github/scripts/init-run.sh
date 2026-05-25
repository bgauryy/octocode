#!/usr/bin/env bash
# init-run.sh — Create a timestamped run directory + measurement log.
# Usage: source init-run.sh <octocode|gh|none>
# Exports: $RUN (absolute path), $LOG (jsonl measurement log)
set -euo pipefail
A="${1:-}"
case "$A" in octocode|gh|none) ;; *) echo "Usage: $0 <octocode|gh|none>" >&2; exit 1 ;; esac
TS=$(date -u +%Y%m%d-%H%M%S)
D="$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")" && pwd)"
RUN="$(cd "$D/.." && pwd)/output/${TS}-${A}"
mkdir -p "$RUN"
for N in $(seq 1 31); do mkdir -p "$RUN/q$N"; done
LOG="$RUN/log.jsonl"
: > "$LOG"
export RUN LOG
echo "$RUN"
