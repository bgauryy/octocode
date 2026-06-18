#!/usr/bin/env bash
# Resumable publisher for the three native packages at 16.3.0.
# Usage: OTP=123456 bash release/publish-16.3.0.sh
# - Publishes in dependency order (platforms before each root).
# - Skips any package already present on the registry (idempotent / resumable).
# - Stops as soon as an OTP expires (EOTP) so you can re-run with a fresh code.
set -u
cd "$(dirname "$0")/.."

OTP="${OTP:-}"
if [[ -z "$OTP" ]]; then echo "Set OTP=<code> before running"; exit 2; fi

# name|dir  in publish order
PKGS=(
  "octocode-security-darwin-arm64|packages/octocode-security/npm/darwin-arm64"
  "octocode-security-darwin-x64|packages/octocode-security/npm/darwin-x64"
  "octocode-security-linux-x64-gnu|packages/octocode-security/npm/linux-x64-gnu"
  "octocode-security-linux-x64-musl|packages/octocode-security/npm/linux-x64-musl"
  "octocode-security-linux-arm64-gnu|packages/octocode-security/npm/linux-arm64-gnu"
  "octocode-security-win32-x64-msvc|packages/octocode-security/npm/win32-x64-msvc"
  "octocode-security|packages/octocode-security"
  "@octocodeai/octocode-context-utils-darwin-arm64|packages/octocode-context-utils/npm/darwin-arm64"
  "@octocodeai/octocode-context-utils-darwin-x64|packages/octocode-context-utils/npm/darwin-x64"
  "@octocodeai/octocode-context-utils-linux-x64-gnu|packages/octocode-context-utils/npm/linux-x64-gnu"
  "@octocodeai/octocode-context-utils-linux-x64-musl|packages/octocode-context-utils/npm/linux-x64-musl"
  "@octocodeai/octocode-context-utils-linux-arm64-gnu|packages/octocode-context-utils/npm/linux-arm64-gnu"
  "@octocodeai/octocode-context-utils-win32-x64-msvc|packages/octocode-context-utils/npm/win32-x64-msvc"
  "@octocodeai/octocode-context-utils|packages/octocode-context-utils"
  "octocode-lsp-darwin-arm64|packages/octocode-lsp/npm/darwin-arm64"
  "octocode-lsp-darwin-x64|packages/octocode-lsp/npm/darwin-x64"
  "octocode-lsp-linux-x64-gnu|packages/octocode-lsp/npm/linux-x64-gnu"
  "octocode-lsp-linux-x64-musl|packages/octocode-lsp/npm/linux-x64-musl"
  "octocode-lsp-linux-arm64-gnu|packages/octocode-lsp/npm/linux-arm64-gnu"
  "octocode-lsp-win32-x64-msvc|packages/octocode-lsp/npm/win32-x64-msvc"
  "octocode-lsp|packages/octocode-lsp"
)

for entry in "${PKGS[@]}"; do
  name="${entry%%|*}"; dir="${entry##*|}"
  if npm view "$name@16.3.0" version >/dev/null 2>&1; then
    echo "SKIP  $name@16.3.0 (already published)"; continue
  fi
  echo "PUB   $name  ($dir)"
  out="$(npm publish "$dir" --access public --otp="$OTP" 2>&1)"
  if [[ $? -eq 0 ]]; then
    echo "OK    $name@16.3.0"
  elif grep -q "EOTP" <<<"$out"; then
    echo "OTP EXPIRED at $name — re-run with a fresh OTP to resume."; exit 10
  else
    echo "$out" | tail -8
    echo "FAIL  $name — stopping."; exit 1
  fi
done
echo "ALL DONE — all 21 packages at 16.3.0 published."
