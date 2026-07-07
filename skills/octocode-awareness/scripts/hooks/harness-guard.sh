#!/usr/bin/env bash
# octocode-awareness harness self-fix gate wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKILL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
[ -f "$ROOT/hook-runner.mjs" ] && OCTOCODE_SKILL_ROOT="$SKILL_ROOT" exec node "$ROOT/hook-runner.mjs" harness-guard
exit 0
