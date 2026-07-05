#!/usr/bin/env bash
# octocode-awareness PreToolUse hook wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts; this file only
# locates the built runner inside the distributed skill scripts directory.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ -f "$ROOT/hook-runner.mjs" ] && exec node "$ROOT/hook-runner.mjs" pre-edit
exit 0
