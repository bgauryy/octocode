#!/usr/bin/env bash
# set-q.sh — Advance the current question. Writes to $RUN/.current-q AND
# $RUN/.q-start (epoch ms at the moment this Q starts).
#
# Usage: bash set-q.sh <n>   # 1..N (N = $RUN/.q-count, written by init-run.sh)
# Env:   RUN (required, exported by init-run.sh)
#
# The two sentinel files:
#   .current-q  — read by mcp-meas.mjs (per tools/call) + gh-meas.sh (per cmd).
#                 Single source of truth for Q routing.
#   .q-start    — read by record.sh. Used to compute q_elapsed_ms = wall time
#                 from this call to record.sh, capturing the AGENT'S TOTAL Q
#                 TIME including reasoning between tool calls (not just the
#                 sum of tool wall times).
set -euo pipefail
: "${RUN:?RUN required}"
N="${1:?usage: set-q.sh <n>}"
[[ "$N" =~ ^[0-9]+$ ]] || { echo "set-q: <n> must be an integer" >&2; exit 1; }
# Upper bound comes from .q-count (written by init-run.sh from the questions file).
# Fallback: if .q-count is absent (legacy run dir), accept any positive integer.
if [[ -f "$RUN/.q-count" ]]; then
  MAX=$(cat "$RUN/.q-count" | tr -d '[:space:]')
  (( N >= 1 && N <= MAX )) || { echo "set-q: <n> must be in 1..$MAX" >&2; exit 1; }
else
  (( N >= 1 )) || { echo "set-q: <n> must be ≥ 1" >&2; exit 1; }
fi
printf '%s\n' "$N" > "$RUN/.current-q"
# Millisecond timestamp: perl first (zero extra node spawn overhead);
# fall back to node if perl is absent (e.g. minimal Docker images).
perl -MTime::HiRes=time -e 'printf "%d\n", time()*1000' 2>/dev/null > "$RUN/.q-start" || \
  node -e 'process.stdout.write(String(Date.now()))' > "$RUN/.q-start"
export Q="$N"
echo "Q=$N"
