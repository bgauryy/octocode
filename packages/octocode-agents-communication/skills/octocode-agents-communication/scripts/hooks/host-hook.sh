#!/bin/sh
# Adapted from octocode-skills/assets/hooks/example-hook.sh.
# Host config owns event selection and a 10-second timeout; Rust owns JSON and DB work.
set -u
case $0 in */*) launcher=${0%/*}/../agents-communication ;; *) launcher=../agents-communication ;; esac
if [ -x "$launcher" ]; then
  # Sourcing keeps one shell hop; the launcher execs the binary.
  set -- host-hook "$@"
  . "$launcher"
fi
printf '%s\n' 'Communication hook: bundled launcher missing' >&2
printf '%s\n' '{}'
