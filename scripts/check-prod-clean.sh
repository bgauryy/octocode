#!/usr/bin/env bash
# Fails if tracked files contain machine-specific paths or dev junk. Read-only.
set -uo pipefail
cd "$(git rev-parse --show-toplevel)"
fail=0
check() { # check <title> <output>
  [ -z "$2" ] && return
  echo "✗ $1"; echo "$2" | head -20 | sed 's/^/    /'; fail=1
}

# Absolute home paths (synthetic fixture users are allowed).
check "absolute home paths" "$(git grep -InE '(/Users/|C:\\Users\\)[A-Za-z0-9._-]+' -- . \
  | grep -vE '/Users/(tester|me|Me|user|u|someone|x)\b|Users\\(someone|me|x|u|tester)\b')"

# Local-machine tmp paths outside tests.
check "scratch paths" "$(git grep -In '/private/tmp/claude' -- . ':!scripts/check-prod-clean.sh')"

# Junk files that should never be tracked.
check "junk files" "$(git ls-files | grep -E '\.(rej|orig|bak|log)$|(^|/)\.DS_Store$|(^|/)(tmp|IMPROVE)/' | grep -v '/fixtures/')"

# file:/// or absolute portal deps in manifests.
check "absolute local deps" "$(git grep -In -E '"(file|portal|link):(///|/)' -- '*package.json')"

[ $fail -eq 0 ] && echo "✓ clean for production"
exit $fail
