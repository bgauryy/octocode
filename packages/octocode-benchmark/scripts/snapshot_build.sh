#!/usr/bin/env bash
# Copy the local MCP build into <dest> so another session's rebuild can't swap or delete it
# mid-campaign: the MCP launcher bundle, and next to it the platform package that carries the
# native binary (`octocode mcp` serves the tools and embeds the contract: descriptions and
# instructions come with it). The launcher resolves the binary from <dest>/node_modules.
# Prints the command that runs the snapshot.
#
#   snapshot_build.sh <dest>
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
mkdir -p "$1"; SNAP="$(cd "$1" && pwd)"
if [ ! -f "$SNAP/octocode-mcp/dist/index.js" ]; then
  PLATFORM="$(node -p 'process.platform+"-"+process.arch')"
  NATIVE="$REPO/packages/octocode-native"
  PKG="$(node -p "require('$NATIVE/bin/platform.cjs').getPlatformSuffix()")"
  SRC="$NATIVE/npm/$PKG"
  BIN="$SRC/octocode"; [ -f "$BIN" ] || BIN="$SRC/octocode.exe"
  [ -f "$BIN" ] || { echo "native binary missing (a build in progress?): rebuild, then retry" >&2; exit 2; }
  DEST="$SNAP/node_modules/@octocodeai/octocode-native-$PKG"
  mkdir -p "$SNAP/octocode-mcp" "$DEST"
  cp -R "$REPO/packages/octocode-mcp/dist" "$REPO/packages/octocode-mcp/package.json" "$SNAP/octocode-mcp/"
  cp "$SRC/package.json" "$BIN" "$DEST/"
  # LSP discovery walks the binary's install ancestors; freeze its bundled TS server too.
  TSGO="$REPO/node_modules/@typescript/native-preview-$PLATFORM"
  [ -d "$TSGO" ] || { echo "bundled TypeScript LSP missing: $PLATFORM" >&2; exit 2; }
  mkdir -p "$SNAP/node_modules/@typescript"
  cp -R "$(cd "$TSGO" && pwd -P)" "$SNAP/node_modules/@typescript/native-preview-$PLATFORM"
  { shasum -a 256 "$DEST"/octocode* "$REPO/packages/octocode-mcp/dist/index.js"; git -C "$REPO" rev-parse HEAD; } > "$SNAP/fingerprint.txt"
  shasum -a 256 "$SNAP/node_modules/@typescript/native-preview-$PLATFORM/lib/tsgo"* >> "$SNAP/fingerprint.txt"
  # The launcher bundle resolves the platform package from the snapshot, not the repo.
  resolved="$(cd "$SNAP/octocode-mcp/dist" && node -e "console.log(require.resolve('@octocodeai/octocode-native-$PKG/package.json'))" 2>/dev/null || true)"
  case "$resolved" in "$(cd "$SNAP" && pwd -P)"/*) ;; *) echo "snapshot not used for @octocodeai/octocode-native-$PKG ($resolved)" >&2; exit 2 ;; esac
fi
echo "node $SNAP/octocode-mcp/dist/index.js"
