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
engine `ToolResult`). Tool descriptions and
schema texts come from `@octocodeai/octocode-core` — this package never invents
its own.

## Tool catalog

`src/tools/toolConfig.ts` is the registry. Each `ToolConfig` declares its name,
category flags (`isLocal`/`isClone`/`isBinary`), a display `schema` + bulk
`inputSchema` (Zod), an `executionFn`, a `security` mode (`basic` | `remote`),
and runtime needs (`requiresServerRuntime`, `requiresProviders`). `ALL_TOOLS`
is the single source of truth.

- **GitHub** (`security: 'remote'`, needs providers): `github_search_code`,
  `github_fetch_content`, `github_view_repo_structure`, `github_search_repos`,
  `github_search_pull_requests`, `github_clone_repo`.
- **Local** (`security: 'basic'`): `local_ripgrep`, `local_find_files`,
  `local_fetch_content`, `local_view_structure`, `local_binary_inspect`.
- **LSP**: `lsp_get_semantic_content` (needs server runtime).
- **Package**: `package_search` (npm).
- **OQL**: `oql_search`, the raw tool entrypoint for the shared OQL runner used
  by CLI `search`.

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
3. **Init runtime** lazily and once: `initialize()` (server config + token) and
   `initializeProviders()`, gated by the tool's `requires*` flags.
4. **Gate** local/clone tools on `ENABLE_LOCAL` / `ENABLE_CLONE` config.
5. **Run** through the security wrapper — `remote` tools get
   `withSecurityValidation` (sanitize + auth + session), `basic` tools get
   `withBasicSecurityValidation`. Both wrappers are thin bridges over
   `octocode-engine/security` in `src/security/bridge.ts`.
6. **Sanitize** the result and always return a structured `CallToolResult` —
   errors become an error envelope (`buildToolErrorResult`), never a throw.

`directToolCatalog.ts` also derives the agent-facing metadata (display fields,
constraints, example queries, auto-filled fields like `id`/`researchGoal`) from
the Zod schemas, and tolerantly drops unknown query fields (warning via
`onUnknownFields`) so a call never hard-fails on a typo.

## Providers

GitHub-only today, behind an `ICodeHostProvider` abstraction so the surface stays
provider-agnostic. `src/providers/factory.ts` caches provider instances
(TTL + LRU, keyed by type/baseUrl/token hash). `src/tools/providerExecution.ts`
builds the execution context from `serverConfig`, runs operations, and
normalizes provider errors. GitHub API plumbing (client, search, content, PRs,
structure, history) lives in `src/github/`.

## Cross-cutting modules

- `src/scheme/` — shared Zod fields and the response envelope.
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

`@octocodeai/octocode-tools-core` is a workspace-only build package. It is not
published to npm and should not appear in any interface package's published
runtime `dependencies`.

- `octocode-mcp` and `octocode` list tools-core as a workspace
  `devDependency` so local builds can import the source package.
- Their esbuild bundles inline tools-core into the shipped `dist/` or `out/`
  entrypoints.
- Runtime dependencies that cannot be bundled — most importantly
  `@octocodeai/octocode-engine` and its native platform packages — are declared
  directly by the interface packages.

This keeps the source ownership centralized here while making npm installs
resolve as `octocode-mcp` / `octocode` → `@octocodeai/octocode-engine` → one
platform `.node` package.

## Rules

- Keep logic here, not in consumers — the CLI/MCP only select and render.
- Descriptions and schema texts come from `octocode-core`; don't hardcode them.
- Native work (minify, search, structural, LSP, masking) goes through
  `octocode-engine`, never reimplemented in TS.
- Add a new tool by adding its `src/tools/<name>/` folder and one `ToolConfig`
  entry in `toolConfig.ts`; everything else (metadata, execution, security) is
  driven off that entry.
