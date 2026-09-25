#!/bin/sh
# Adapted from octocode-skills/assets/hooks/example-hook.sh.
# Host config owns event selection and a 10-second timeout; Rust owns JSON and DB work.
set -u
ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)" || exit 0
if [ -x "$ROOT/agents-communication" ]; then
  exec "$ROOT/agents-communication" host-hook "$@"
fi
printf '%s\n' 'Communication hook: bundled launcher missing' >&2
printf '%s\n' '{}'
