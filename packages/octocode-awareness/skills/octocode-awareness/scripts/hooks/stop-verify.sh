#!/usr/bin/env bash
# octocode-awareness Stop/SubagentStop hook wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNNER="$ROOT/hook-runner.mjs"
NODE_BIN="${OCTOCODE_NODE_BIN:-node}"
[ -f "$RUNNER" ] && exec "$NODE_BIN" "$RUNNER" stop-verify
echo "octocode-awareness: missing hook runner at $RUNNER; stop-verify hook skipped. Rebuild or reinstall octocode-awareness hooks." >&2
exit 0
