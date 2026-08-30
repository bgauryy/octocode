# Octocode MCP Architecture

`octocode-mcp` is a **thin MCP server**. It owns process lifecycle, tool
registration, and output safety. It owns **no** business logic, schemas, or tool
metadata — those live in `@octocodeai/octocode-tools-core` (`toolContract/` for
schemas/descriptions and `octocode-engine` for native search/minify/LSP). The
MCP package wires those into the MCP SDK and runs them.

## Boundary

- **Transport**: MCP SDK v2 `StdioServerTransport` from
  `@modelcontextprotocol/server/stdio` (stdio only — no HTTP).
- **Entry**: `src/index.ts` → `startServer()` → stdio transport.
- **Public API**: `src/public.ts` is the stable surface for programmatic
  consumers (`octocode` CLI, `octocode-research`). It mostly re-exports core
  types, schemas, and `execute*` runners. Internal `register*Tool` functions are
  intentionally absent.
- Logic, descriptions, schemas, and the system prompt come from core. Do not
  add data-shaping here — shape it in core.

## Startup (`src/index.ts`)

`startServer()` runs a fixed sequence, then connects the transport:

1. `initialize()` — core bootstrap, including the persisted cache-maintenance due-check.
2. `configureSecurity()` + register `getOctocodeDir()` as an allowed root.
3. `initializeProviders()` — GitHub / GitLab / Bitbucket.
4. `loadToolContent()` — pull descriptions/metadata from core.
5. `initializeSession()`, then `createServer()` + `registerAllTools()`.

After the transport connects, `startCacheGC()` schedules the next persisted maintenance deadline with an unreferenced timer. This is deadline-based scheduling from the shared 24-hour marker, not a fresh interval measured from each MCP start, so CLI and MCP processes observe the same gate. A cross-process lock prevents overlapping sweeps.

The server is created with `instructions: completeMetadata.systemPrompt` (core).
Process handlers wire SIGINT/SIGTERM/STDIN-close/uncaught/unhandled to a single
`gracefulShutdown` that stops the cache-maintenance scheduler, clears runtime caches, and closes the server
within `SHUTDOWN_TIMEOUT_MS` (5s) before exiting.

## Tool Registration (`src/tools/`)

- `toolConfig.ts` — joins core's `ALL_TOOLS` metadata with this package's
  `register*` functions via `MCP_FN_MAP`, producing `ALL_TOOLS: McpToolConfig[]`.
  A tool in core with no MCP `fn` is a build-time error.
- `toolsManager.ts` — `registerTools()`: wraps the server with output
  sanitization, filters tools (local/clone gates + filter config), validates
  metadata, then batch-registers and summarizes outcomes.
- `registrationExecutor.ts` — registers tools in parallel; per-tool failures are
  isolated and reported (`success` / `failed` / `skipped`), never fatal unless
  zero tools register.
- `toolFilters.ts` — `isLocal`/`isClone` capability gates plus catalog-level
  `ENABLE_RELEASES`/`ENABLE_DISCUSSIONS` gates and
  `TOOLS_TO_RUN` (exclusive) vs `DISABLE_TOOLS` selection;
  `isDefault` tools register unless disabled.
- `metadataPolicy.ts` — a tool is skipped (not failed) if core has no valid
  metadata for it, unless `skipMetadataCheck`.

### Two registration shapes

- `registerBasicTool.ts` — local single-call tools. Wraps the core `execute*`
  with `withBasicSecurityValidation`. Defaults: `readOnlyHint`, `openWorldHint: false`.
- `registerRemoteTool.ts` — bulk `queries[]` tools (GitHub/npm). Wraps with
  `withSecurityValidation`, forwards `responseCharOffset/Length`, supports an
  optional async `registrationGuard` (skip if preconditions unmet) and a
  `describe()` hook. Default `openWorldHint: true`.

Each `src/tools/<tool>/` file is a few lines: name + title + core schema + core
`execute*` runner. The tool families are: **GitHub** (search code, fetch
content, view repo structure, search repos, search PRs, clone repo),
**package** (npm search), **local** (ripgrep, view structure, find files, fetch
content, binary inspect), and **LSP** (semantic content).

## Output Safety (`src/utils/secureServer.ts`)

`withOutputSanitization()` is a `Proxy` over `McpServer` that intercepts
`registerTool`/`registerResource`. Every tool callback is wrapped so its result
is passed through `sanitizeCallToolResult` (secret masking via
`ContentSanitizer` + `maskSensitiveData`), and thrown errors are normalized,
sanitized, and converted to a safe tool error result instead of crashing the
server.

## Schema Bridging (`src/types/toolTypes.ts`)

`toMCPSchema()` unwraps Zod `pipe`/`ZodEffects`/`ZodPipeline` wrappers to the
inner object schema before handing it to the MCP SDK — avoids exponential type
inference from the SDK's Zod v3/v4 compat layer.

## Dependencies

There are two different dependency views:

- **Source/build**: `@octocodeai/octocode-tools-core` resolves to the workspace
  during local development. The MCP source imports its runners, schemas,
  metadata, and shared utilities; `buildConfig.mjs` keeps runtime dependencies
  external in `dist/index.js` and `dist/public.js`.
- **Published runtime**: npm users install the direct dependencies declared in
  `package.json`: MCP SDK v2's `@modelcontextprotocol/server`, tools-core, core,
  the native engine, and Zod. Client SDK v2 is test-only.
- **Native engine**: `@octocodeai/octocode-engine` must remain a direct runtime
  dependency because its Rust `.node` binary is distributed through the engine
  root package plus one matching platform `optionalDependency`.
- **Types**: `dist/public.d.ts` is bundled so public declarations do not leak a
  dependency on the unpublished tools-core package.

Publish order follows the runtime graph: publish the engine platform packages,
then the engine root and tools-core, then `octocode-mcp`.

## Distribution Artifacts

`package.json#files` ships four generated/static files alongside `dist/`:

- `README.md` — **not** hand-authored here. `yarn readme:sync` (runs before
  `build`/`build:dev`/`prepack`) copies the root `README.md` in via
  `scripts/sync-package-readmes.mjs`. Edit the root `README.md`, never this
  package's copy directly — it is gitignored and overwritten on every sync.
- `manifest.json` — Claude Desktop / DXT extension manifest. Declares the
  `mcp_octocode_*`-prefixed tool catalog, `user_config` fields, and platform
  compatibility for the desktop-extension install path.
- `server.json` — MCP registry submission (`io.github.bgauryy/octocode-mcp`),
  validated against the `2025-10-17` server schema. Declares the npm package
  identifier/version the registry resolves and the `env` vars a client may set.
- `LICENSE` — MIT, copied as-is.

`manifest.json` and `server.json` are **hand-maintained** — no build step syncs
their `version` or tool list from `package.json` or core's `ALL_TOOLS`. When
the tool catalog changes or a release ships, update both files' `version`
fields and `manifest.json#tools` alongside `package.json#version` and the
`toolConfig.ts` `MCP_FN_MAP`; drift here does not fail CI and will only
surface as an outdated registry/DXT listing.
