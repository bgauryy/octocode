# Octocode Tools Core Architecture

`@octocodeai/octocode-tools-core` is the **tool execution layer** of Octocode. It
owns all the logic — schemas, provider calls, file/LSP operations, response
shaping, pagination, hints, security, credentials, config, and session state.
Consumers (the `octocode` CLI and `octocode-mcp` server) are thin: they pick a
tool, hand it input, and render the `CallToolResult` it returns.

Native heavy lifting (minify, local search, structural AST, secret
detection/masking) and LSP orchestration (client pool, resolver, security
validation) are delegated to `@octocodeai/octocode-engine` — a Rust/napi core
plus a TS orchestration layer. tools-core reaches the Rust core through the
lazy `contextUtils` proxy (`src/utils/contextUtils.ts`) and the TS wrappers
through the `./lsp/*` / `./security/*` subpath exports; its own
`src/security/bridge.ts` is only a thin type adapter (MCP `CallToolResult` ↔
engine `ToolResult`). Tool descriptions and executable schemas are owned by
`src/toolContract/`; the external core package supplies the shared system
prompt and reusable output types.

## Tool catalog

`src/tools/toolConfig.ts` is the registry. Each `ToolConfig` declares its name,
category flags (`isLocal`/`isClone`), a display `schema` + bulk
`inputSchema` (Zod), an `executionFn`, a `security` mode (`basic` | `remote`),
and runtime needs (`requiresServerRuntime`, `requiresProviders`). `ALL_TOOLS`
is the single source of truth.

- **GitHub** (`security: 'remote'`, needs providers): `ghSearch`,
  `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`,
  `ghListReleases`, `ghSearchDiscussions`, and `ghCloneRepo`.
- **Package**: `npmSearch`.
- **Local** (`security: 'basic'`): `localSearch`, `localGetFileContent`, and
  `localAnalyzeGraph`.
- **LSP**: `lspGetSemantics` (needs server runtime).

Each tool lives in `src/tools/<tool_name>/` with a common core — `scheme.ts`
(Zod single + bulk schemas) and `execution.ts` (the bulk-loop `executionFn`) —
plus `finalizer.ts` / `types.ts` and helper modules as needed (the set varies
per tool; e.g. `local_ripgrep` splits ranking/structural/executor into separate
files). Next-step hints are generated centrally by
`src/utils/pagination/hints.ts`, not per tool.

## Execution flow

`executeDirectTool(name, input)` in `src/tools/directToolCatalog.ts` is the entry
point used by all consumers:

1. **Resolve** the tool from `ALL_TOOLS`.
2. **Parse** input against `inputSchema` (bulk `{ queries: [...] }`).
3. **Bootstrap cache maintenance** with a cheap persisted due-check once per
   process. Non-server local tools enter it directly; server-requiring tools
   enter it through `initialize()`.
4. **Init runtime** lazily and once: `initialize()` (server config + token) and
   `initializeProviders()`, gated by the tool's `requires*` flags.
5. **Gate** local/clone tools on `ENABLE_LOCAL` / `ENABLE_CLONE` config.
6. **Run** through the security wrapper — `remote` tools get
   `withSecurityValidation` (sanitize + auth + session), `basic` tools get
   `withBasicSecurityValidation`. Both wrappers are thin bridges over
   `octocode-engine/security` in `src/security/bridge.ts`.
7. **Sanitize** the result and always return a structured `CallToolResult` —
   errors become an error envelope (`buildToolErrorResult`), never a throw.

`directToolCatalog.ts` also derives agent-facing fields, variants, relations,
and examples from the Zod schemas. Public query envelopes are strict: unknown
fields fail before execution with a correction hint.

## Providers

GitHub-only today, behind an `ICodeHostProvider` abstraction so the surface stays
provider-agnostic. `src/providers/factory.ts` caches provider instances
(TTL + LRU, keyed by type/baseUrl/token hash). `src/tools/providerExecution.ts`
builds the execution context from `serverConfig`, runs operations, and
normalizes provider errors. GitHub API plumbing (client, search, content, PRs,
structure, history) lives in `src/github/`.

## Cross-cutting modules

- `src/cacheMaintenance.ts` — shared 24-hour maintenance gate, persisted marker,
  cross-process lock, owned-root sweep, and MCP deadline scheduler for
  `tmp/clone`, `tmp/tree`, and `tmp/response`.
- `src/scheme/` — shared Zod input fields. Output schemas are not published.
- `src/utils/pagination/` (incl. `hints.ts` — next-step hints: pagination
  cursors, token-budget warnings, structure hints) + `src/utils/response/` — the
  single lossless char-pagination flow and YAML/JSON result rendering shared by
  text + `structuredContent`.
- `src/utils/{http,exec,file,package,parsers}/` — fetch+retry+cache+circuit
  breaker, safe `spawn`, file helpers, npm, ripgrep/diff parsers.
- `src/errors/` — `ToolError` hierarchy and domain/local error factories.
- `src/shared/` — `config`, `credentials` (token storage/refresh/env/gh-cli),
  `session` (stats), `platform`, `paths`. These are exported as
  subpath entry points (`./config`, `./credentials`, `./session`, …).

## Public surface

- `src/index.ts` — the full re-export barrel (everything above + selected
  `octocode-engine` and `octocode-core` re-exports).
- `src/direct.ts` — the minimal `./direct` entry: `executeDirectTool` plus the
  catalog/metadata helpers consumers need to drive tools.
- `src/zod.ts`, `./platform`, `./session`, `./config`, `./credentials`,
  `./paths`, `./fs-utils`, `./testing` — focused subpath entries (see
  `package.json#exports`).

## Distribution

`@octocodeai/octocode-tools-core` is a published runtime package with public
entry points in `package.json#exports`. Workspace consumers resolve its source
during development.

- `octocode-mcp` keeps tools-core external and declares it as a runtime
  dependency; npm installs it with the server.
- `octocode` also declares tools-core as a runtime dependency, while its custom
  esbuild source alias bundles tools-core code into `out/octocode.js` for the CLI.
- Native dependencies, especially `@octocodeai/octocode-engine` and its matching
  platform package, remain external.

Publish runtime prerequisites before their consumers: engine platform packages,
the engine root, config/core/tools-core, and then the CLI and MCP interfaces.

## Rules

- Keep logic here, not in consumers — the CLI/MCP only select and render.
- Descriptions and schemas come from `src/toolContract/`; don't hardcode them in interfaces or runners.
- Native work (minify, search, structural, LSP, masking) goes through
  `octocode-engine`, never reimplemented in TS.
- Add a new tool by adding its `src/tools/<name>/` folder and one `ToolConfig`
  entry in `toolConfig.ts`; everything else (metadata, execution, security) is
  driven off that entry.
