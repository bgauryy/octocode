#!/usr/bin/env bash
# octocode-awareness PreToolUse hook wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts; this file only
# locates the built runner inside the distributed skill scripts directory.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNNER="$ROOT/hook-runner.mjs"
[ -f "$RUNNER" ] && exec node "$RUNNER" pre-edit
echo "octocode-awareness: missing hook runner at $RUNNER; pre-edit hook skipped. Rebuild or reinstall octocode-awareness hooks." >&2
exit 0
