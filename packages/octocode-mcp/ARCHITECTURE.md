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
manifests and the logo. Its `optionalDependencies` pin every
`@octocodeai/octocode-native-<platform>` package in `PLATFORMS` at the release
version, so `npx octocode-mcp` installs the one binary for the machine; when
npm skipped it, the shared launcher code fetches it once from the registry
(see the `octocode` package's ARCHITECTURE.md). Publish the native platform
packages before `octocode-mcp`.

`package.json` defines the build and verification entry points. Checks:

- `tests/launcher/*` (vitest): binary resolution, argv and exit-code
  forwarding, and the startup-failure reply;
- `yarn test:boundary`: the shared hermetic `transport.mjs` suite checks CLI
  and MCP admission, complete Unicode source walks, opaque cursors and replay
  authority through the built launchers;
- `node skills-dev/octocode-dev/scripts/dev.mjs test:tools` (repo root): the shared
  `tool-health.mjs` suite, which executes all 14 tools through CLI and MCP with loopback providers,
  output validation, typed continuations and evidence parity;
- `yarn test:contracts:classification`: classification capability publication,
  selection, provider failure and complete unfiltered recovery against a loopback scorer;
- native Rust tests (`crates/cli/tests/mcp_server.rs` and `listing/tests.rs`
  under `packages/octocode-native`) for protocol eras, listing and shutdown.

The shared suites own their isolated homes, source fixtures and mock providers.
No registered acceptance command sends evidence to a paid provider.
