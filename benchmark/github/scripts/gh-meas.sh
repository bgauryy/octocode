#!/usr/bin/env bash
# gh-meas.sh — Thin wrapper. Delegates to gh-meas.mjs.
#
# gh-meas.mjs runs timing and Unicode codepoint counting in one Node
# process, avoiding extra subprocess timing overhead.
#
# Usage: bash gh-meas.sh <gh args...>   (same as before — no change needed)
# Env:   LOG, RUN (required, same as before)
exec node "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"; pwd)/gh-meas.mjs" "$@"
