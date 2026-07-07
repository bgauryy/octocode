#!/usr/bin/env bash
# octocode-awareness PostToolUse hook wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -f "$ROOT/hook-runner.mjs" ] && exec node "$ROOT/hook-runner.mjs" post-edit
exit 0
