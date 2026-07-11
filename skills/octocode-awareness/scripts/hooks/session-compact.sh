#!/usr/bin/env bash
# octocode-awareness PreCompact hook wrapper.
# Finalizes the current fallback run without ending the reusable host session.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNNER="$ROOT/hook-runner.mjs"
NODE_BIN="${OCTOCODE_NODE_BIN:-node}"
[ -f "$RUNNER" ] && exec "$NODE_BIN" "$RUNNER" session-compact
echo "octocode-awareness: missing hook runner at $RUNNER; pre-compact hook skipped. Rebuild or reinstall octocode-awareness hooks." >&2
exit 0
