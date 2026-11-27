#!/usr/bin/env bash
# shellcheck disable=SC2034
set -euo pipefail

# a comment
name="a  b"   # trailing
echo "# not comment"
echo it\'s # c
cat <<EOF2
# heredoc line, not comment
  keep   spaces
EOF2
count=${#name}
echo $# items # n
if [ -n "$name" ]; then
    # inner
    echo ok
fi
