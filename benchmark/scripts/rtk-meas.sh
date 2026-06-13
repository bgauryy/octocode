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
#   bash rtk-meas.sh rg 'notFound' /tmp/nextjs-bench/packages/next/src
#   bash rtk-meas.sh read /tmp/nextjs-bench/packages/next/src/server/base-server.ts
#   bash rtk-meas.sh ls /tmp/nextjs-bench/packages/next/src/server
#   bash rtk-meas.sh find /tmp/nextjs-bench/packages/next/src --name '*.ts'
exec node "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"; pwd)/rtk-meas.mjs" "$@"
