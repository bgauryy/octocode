# Octocode Releases

Self-contained executables (Node SEA): Node runtime + JS bundle + native Rust engine in one file.
Nothing to install on the target machine — no Node, no npm, no npx.

```
releases/
  mcp/octocode-mcp-<platform>   MCP stdio server
  cli/octocode-<platform>       octocode CLI
```

Binaries are gitignored; only this README is tracked.
Built by [`scripts/release.mjs`](../scripts/release.mjs).

## Make a new release

```bash
# 1. Engine binaries (once per engine change)
yarn build:native:all

# 2. Build both executables (current platform)
yarn release
```

Individually: `yarn workspace octocode-mcp release` · `yarn workspace octocode release`

Other platforms (darwin-x64, linux-x64, linux-x64-musl, linux-arm64, windows-x64):
build on a matching CI runner, or pass `--node-bin <target-node>` to `scripts/release.mjs`.

## Publish to GitHub

Choose an unused release version, complete the repository publish checks, and
then create the tag and release through the normal release process. Upload the
matching files from `releases/mcp/` and `releases/cli/`.

Users then install with:

```bash
curl -fsSL -o octocode-mcp \
  https://github.com/bgauryy/octocode/releases/latest/download/octocode-mcp-darwin-arm64
chmod +x octocode-mcp
```

## Use

MCP (any client):

```bash
claude mcp add octocode -- /absolute/path/to/octocode-mcp-<platform>
```

```json
{ "mcpServers": { "octocode": { "command": "/absolute/path/to/octocode-mcp-<platform>" } } }
```

CLI:

```bash
./octocode-<platform> tools                    # list tools
./octocode-<platform> tools localSearch --scheme --compact
./octocode-<platform> tools localSearch --queries '{"operation":"text","path":"/ABS/repo","searchText":"symbol"}' --compact
```

Local tools work with zero setup. GitHub tools read `GITHUB_TOKEN` / `GH_TOKEN` / `OCTOCODE_TOKEN`.
