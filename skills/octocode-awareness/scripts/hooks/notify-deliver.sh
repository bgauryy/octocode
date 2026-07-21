#!/usr/bin/env bash
# octocode-awareness UserPromptSubmit hook wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNNER="$ROOT/hook-runner.mjs"
NODE_BIN="${OCTOCODE_NODE_BIN:-node}"
[ -f "$RUNNER" ] && exec "$NODE_BIN" "$RUNNER" notify-deliver
echo "octocode-awareness: missing hook runner at $RUNNER; notify-deliver hook skipped. Rebuild or reinstall octocode-awareness hooks." >&2
exit 0
