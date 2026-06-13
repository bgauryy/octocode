#!/usr/bin/env bash
# octo-meas.sh — Thin wrapper. Delegates to octo-meas.mjs.
#
# Timing and Unicode codepoint counting run in-process (Node), avoiding extra
# subprocess timing overhead — same pattern as gh-meas.sh → gh-meas.mjs.
#
# Usage: bash octo-meas.sh <tool-name> '<queries-json>'
# Env:   LOG, RUN (required, exported by init-run.sh)
exec node "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"; pwd)/octo-meas.mjs" "$@"
