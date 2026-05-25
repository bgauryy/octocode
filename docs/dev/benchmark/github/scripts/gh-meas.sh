#!/usr/bin/env bash
# gh-meas.sh — Thin wrapper. Delegates to gh-meas.mjs.
#
# The original shell implementation spawned Node 5 times per call
# (2× Date.now(), 2× chars.mjs, 1× JSON log), adding ~100–200 ms of
# systematic overhead to tool_elapsed_ms and biasing timing against gh.
# gh-meas.mjs runs entirely in one Node process with no spawning overhead.
#
# Usage: bash gh-meas.sh <gh args...>   (same as before — no change needed)
# Env:   LOG, RUN (required, same as before)
exec node "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"; pwd)/gh-meas.mjs" "$@"
