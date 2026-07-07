#!/usr/bin/env bash
# Template hook wrapper — see references/hooks.md before adapting.
#
# Copy this file into the target skill's scripts/hooks/<name>.sh, then wire it
# from that skill's SKILL.md frontmatter as:
#   command: "${CLAUDE_SKILL_DIR}/scripts/hooks/<name>.sh"
#
# Keep all real logic in the companion brain script (example-hook-brain.mjs).
# This wrapper only self-locates the skill root and execs the brain script —
# never inline decision logic here.
set -uo pipefail

# Assumes this file lives at scripts/hooks/<name>.sh, so ".." is scripts/.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BRAIN="$ROOT/example-hook-brain.mjs"

# Replace "check" with whatever subcommand the brain script expects for this event.
if [ -f "$BRAIN" ]; then
  exec node "$BRAIN" check
fi

# Fail open: a missing/misconfigured brain script must never block real work.
exit 0
