#!/usr/bin/env bash
# gh-meas.sh — Thin wrapper around the GitHub CLI. Delegates to gh-meas.mjs.
#
# Measures Unicode codepoints in/out and logs to $LOG.
#
# Usage: bash gh-meas.sh <gh-subcommand-and-flags>
# Env:   LOG, RUN (required, exported by init-run.sh)
#
# Examples:
#   bash gh-meas.sh api repos/vercel/next.js/contents/packages
#   bash gh-meas.sh search code 'notFound repo:vercel/next.js' --json repository,path,textMatches
#   bash gh-meas.sh pr view 12345 --repo vercel/next.js --json title,body,comments
exec node "$(cd "$(dirname "${BASH_SOURCE[0]:-$0}")"; pwd)/gh-meas.mjs" "$@"
