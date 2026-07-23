# Octocode Single-File Releases

Self-contained executables built with [Node SEA](https://nodejs.org/api/single-executable-applications.html)
(Single Executable Applications). Each binary bundles the full JS program, the
Node 22 runtime, and the `@octocodeai/octocode-engine` native Rust addon —
**no Node, npm, or npx required on the target machine**.

```
releases/
  mcp/octocode-mcp-<platform>   MCP stdio server (@octocodeai/mcp)
  cli/octocode-<platform>       octocode CLI
```

Platforms: `darwin-arm64` · `darwin-x64` · `linux-x64` · `linux-x64-musl` · `linux-arm64` · `windows-x64` (`.exe`)

Binaries in this folder are gitignored — only this README is tracked.

## How to build

Everything is driven by [`scripts/release.mjs`](../scripts/release.mjs).

```bash
# 1. One-time prerequisites
yarn install                     # workspace deps (esbuild, postject fallback via npx)
yarn build:native:all            # engine .node binaries for all platforms (or a single
                                 #   yarn workspace @octocodeai/octocode-engine build:darwin-arm64)

# 2. Build both executables for the current host platform
yarn release

# — or individually —
yarn workspace @octocodeai/mcp release        # → releases/mcp/
yarn workspace octocode release               # → releases/cli/
```

Direct script usage (all flags optional):

```bash
node scripts/release.mjs --target mcp|cli \
  --platform darwin-arm64 \
  --node-bin /path/to/target/node \
  --keep-workdir
```

### What the build does

1. Generates a SEA entry wrapper that extracts the engine addon to
   `~/.octocode/native/<engine-version>/` on first run (native code cannot be
   dlopen'd from inside an executable) and publishes the pre-loaded binding as
   `globalThis.__OCTOCODE_ENGINE_BINDING__` — every engine load site checks it first.
2. Bundles wrapper + program into one CJS file with esbuild
   (tools-core inlined from source, same as the CLI build — no stale-dist trap).
3. Builds the SEA blob (`node --experimental-sea-config`) with the platform's
   engine `.node` as an embedded asset.
4. Copies the target `node` binary, injects the blob with `postject`,
   and (macOS) re-signs with `codesign -s -`.

### Cross-platform builds

The blob is injected into a **node binary for the target platform**. By default
the current `node` is used, so a local build only produces the host platform.
Other platforms are built natively on a CI runner matrix, or by passing
`--node-bin` a downloaded official Node binary for the target
(macOS targets must be signed on macOS, or with `rcodesign` elsewhere).

The engine `.node` for the platform must exist under
`packages/octocode-engine/npm/<triple>/` (built by `build:native:all` / napi CI).

## How to use

MCP (any client — Claude Code, Claude Desktop, Cursor, …):

```bash
claude mcp add octocode -- /absolute/path/to/releases/mcp/octocode-mcp-darwin-arm64
```

```json
{ "mcpServers": { "octocode": { "command": "/absolute/path/to/octocode-mcp-darwin-arm64" } } }
```

CLI:

```bash
./releases/cli/octocode-darwin-arm64 tools
```

Local tools work with zero setup. GitHub tools read `OCTOCODE_TOKEN` /
`GH_TOKEN` / `GITHUB_TOKEN` (or fall back to `gh auth token`) exactly like the
npm distribution — pass an `env` block in the MCP config if the client does not
inherit your shell environment.
