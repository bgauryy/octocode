#!/usr/bin/env bash
# Remove expired or age-stale awareness file locks.
# Usage:
#   scripts/prune-stale-locks.sh [minutes] [awareness.py prune-stale-locks flags...]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
minutes="${OCTOCODE_STALE_LOCK_MINUTES:-20}"
if [ "${1:-}" != "" ] && [[ "${1:-}" =~ ^[0-9]+$ ]]; then
  minutes="$1"
  shift
fi

exec python3 "$ROOT/awareness.py" prune-stale-locks --older-than-minutes "$minutes" "$@"
