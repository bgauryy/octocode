# Octocode MCP architecture

`octocode-mcp` is a thin Node launcher for the MCP server that lives in the
native binary (`octocode mcp`, `@octocodeai/octocode-native`). It owns process
startup only. It does not frame the protocol, list, select, sanitize or execute
tools, in TypeScript or anywhere else outside the binary.

```mermaid
flowchart LR
  client[MCP client] -- stdio --> launcher["octocode-mcp (Node launcher)"]
  launcher -- "same stdio, argv: mcp" --> bin["octocode mcp (native binary)"]
  bin --> runtime[ToolRuntime]
```

## Runtime boundary

- `src/index.ts` resolves the platform binary and runs `octocode mcp [args]`
  with inherited stdio. Resolution, signal forwarding and the child's exit code
  come from the `octocode` CLI launcher (`packages/octocode/src/cli/native-delegate.ts`),
  so both packages share one implementation. `OCTOCODE_NATIVE_BIN` points the
  launcher at a candidate binary; the bundled build honors it only with
  `NODE_ENV=development` or `test`.
- `src/startup-failure.ts` covers the one failure the binary cannot report: it
  never started (platform package missing, binary not executable). The launcher
  answers the client's first JSON-RPC request with the reason as a `-32603`
  error, then exits 1, so the client shows why instead of a closed connection.
- `build.mjs` bundles `src/index.ts` into `dist/index.js`. `@octocodeai/config`
  is a private workspace package, so it is bundled at build time (the shared
  launcher uses its development-override guard).
- The native binary owns protocol framing for both eras (2025-11-25
  `initialize`, 2026-07-28 `server/discover`), the tool list, descriptions and
  instructions (the contract embedded at build time), configuration, policy,
  validation, execution, response shaping, pagination, cancellation and
  shutdown. Its design: `packages/octocode-native/ARCHITECTURE.md`
  (`crates/cli/src/cli/mcp_server/`); behavior and tool catalog: `docs/OCTOCODE_MCP.md`.

There is no TypeScript tool fallback and no addon. If the platform binary is
missing, startup fails closed with the reason above.

## Lifecycle

The launcher passes stdin, stdout and stderr straight to the binary and
forwards SIGINT and SIGTERM. The binary cancels in-flight requests, closes the
runtime and exits; the launcher exits with the same code.

## Distribution

The package ships the bundled launcher (`dist/index.js`), `dist/docs/`, the
manifests and the logo. Its `optionalDependencies` pin the seven
`@octocodeai/octocode-native-<platform>` packages at the release version, so
`npx octocode-mcp` installs the one binary for the machine. Publish the
native platform packages before `octocode-mcp`.

`package.json` defines the build and verification entry points. Checks:

- `tests/launcher/*` (vitest): binary resolution, argv and exit-code
  forwarding, and the startup-failure reply;
- `yarn test:boundary` (`tests/native/response-pagination.mjs`): response and
  tool pagination through the native CLI, the npm CLI launcher and the MCP
  server, stale-cursor rejection and Unicode;
- `yarn test:acceptance` (`tests/integration/stdio.acceptance.mjs`): the built
  launcher over real stdio with the v2 MCP client, and CLI/MCP parity
  (`--cli-mcp-parity`); the runner takes its shared client and receipt from
  `stdio-harness.mjs` and its checks from the `stdio-*-checks.mjs` modules;
- `yarn test:contracts:classification`: classification availability and
  context-selection scenarios;
- native Rust tests (`crates/cli/tests/mcp_server.rs` and `listing/tests.rs`
  under `packages/octocode-native`) for protocol eras, listing and shutdown.

Acceptance fixtures live in a normal workspace subtree and are removed after
the run; placing them under a Git-ignored directory would test ignore rules
instead of discovery and rewriting.
