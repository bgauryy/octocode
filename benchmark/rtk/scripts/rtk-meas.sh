#!/usr/bin/env bash
# rtk-meas.sh — Thin wrapper. Delegates to rtk-meas.mjs.
#
# Runs timing and Unicode codepoint counting in one Node process,
# mirroring the same ruler used by gh-meas.sh.
#
# Usage: bash rtk-meas.sh <rtk args...>
# Env:   LOG, RUN (required — set by init-run.sh)
#
# Examples:
#   bash rtk-meas.sh rg 'fn run' /tmp/rtk-bench/src
#   bash rtk-meas.sh read /tmp/rtk-bench/src/core/runner.rs
#   bash rtk-meas.sh ls /tmp/rtk-bench/src
#   bash rtk-meas.sh find /tmp/rtk-bench/src --name '*.rs'
#   bash rtk-meas.sh gh pr view 2129 --repo rtk-ai/rtk
exec node "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"; pwd)/rtk-meas.mjs" "$@"
