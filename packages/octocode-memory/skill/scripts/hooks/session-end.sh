#!/usr/bin/env bash
# octocode-awareness SessionEnd hook wrapper.
# Logic lives in packages/octocode-memory/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -f "$ROOT/hook-runner.mjs" ] && exec node "$ROOT/hook-runner.mjs" session-end
exit 0
