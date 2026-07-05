#!/usr/bin/env bash
# octocode-awareness Stop/SubagentStop hook wrapper.
# Logic lives in packages/octocode-memory/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -f "$ROOT/hook-runner.mjs" ] && exec node "$ROOT/hook-runner.mjs" stop-verify
exit 0
