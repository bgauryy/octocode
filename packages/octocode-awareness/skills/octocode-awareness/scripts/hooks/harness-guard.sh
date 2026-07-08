#!/usr/bin/env bash
# octocode-awareness harness self-fix gate wrapper.
# Logic lives in packages/octocode-awareness/bin/hook-runner.ts.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SKILL_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNNER="$ROOT/hook-runner.mjs"
[ -f "$RUNNER" ] && OCTOCODE_SKILL_ROOT="$SKILL_ROOT" exec node "$RUNNER" harness-guard
echo "octocode-awareness: missing hook runner at $RUNNER; harness-guard hook skipped. Rebuild or reinstall octocode-awareness hooks." >&2
exit 0
