# Octocode tools-core architecture

`@octocodeai/octocode-tools-core` is the **tool execution layer** of Octocode. It
owns runtime logic — provider calls, file/LSP operations, response
shaping, pagination, hints, security, credentials, config, and session state.
Consumers (the `octocode` CLI and `octocode-mcp` server) are thin: they pick a
tool, hand it input, and render the `CallToolResult` it returns.

Native heavy lifting (minify, local search, batched graph-fact scanning,
structural AST, secret detection/masking) and LSP orchestration (client pool, resolver, security
validation) are delegated to `@octocodeai/octocode-engine` — a Rust/napi core
plus a TS orchestration layer. tools-core reaches the Rust core through the
lazy `contextUtils` proxy (`src/utils/contextUtils.ts`) and the TS wrappers
through the `./lsp/*` / `./security/*` subpath exports; its own
`src/security/bridge.ts` is only a thin type adapter (MCP `CallToolResult` ↔
engine `ToolResult`). The sibling `@octocodeai/octocode-core` package owns
tool names, descriptions, executable schemas, relations, and reusable output
types. Import contracts from its `/schema` entrypoint and shared agent
instructions from `/mcp`.

## Tool catalog

`src/tools/toolConfig.ts` is the registry. Each `ToolConfig` declares its name,
category flags (`isLocal`/`isClone`), a display `schema` + bulk
`inputSchema` (Zod), an `executionFn`, a `security` mode (`basic` | `remote`),
and runtime needs (`requiresServerRuntime`, `requiresProviders`). `ALL_TOOLS`
attaches runtime behavior to core's canonical catalog.

- **GitHub** (`security: 'remote'`, needs providers): `ghSearch`,
  `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`,
  and `ghCloneRepo`.
- **Package**: `artifactSearch`.
- **Local** (`security: 'basic'`): `localSearch`, `astSearch`, `astRewrite`,
  and `localFetch`.
- **LSP**: `lspSearch`; its engine-managed local client pool does not initialize
  GitHub providers or the server runtime.

Each tool lives in `src/tools/<tool_name>/` with `execution.ts` (the bulk-loop
`executionFn`), plus `finalizer.ts` / `types.ts` and helper modules as needed.
Handlers import the canonical executable schemas from core. The public
`astSearch` dispatcher is `src/tools/ast_search/execution.ts`: it validates the
operation union and routes `match`, `files`, `tree`, `symbols`, and `topology`.
Topology execution and graph-analysis policy belong to
`src/tools/ast_search/topology/`; the dispatcher keeps any lower-level search,
filesystem, or AST helpers private behind that public contract. File discovery
and filesystem tree handlers consume the AST operation inputs directly and
emit `astSearch` continuations. Rust `queryFileSystem` owns traversal; tools-core
owns sorting, response shaping, and scan-limit reporting. Next-step hints
are generated centrally by `src/utils/pagination/hints.ts`, not per tool.

### Graph ownership

`src/graph/buildFileGraph.ts` owns file-level import-edge construction and
`src/graph/reachability.ts` owns shared traversal/SCC primitives. The bounded
topology analyses, dead-code policy, pagination, and response shaping are owned
by `src/tools/ast_search/topology/`. `astSearch` is the public graph surface;
private graph helpers and harnesses are implementation details and are not
separate catalog tools.

## Execution flow

`executeDirectTool(name, input)` in `src/tools/directToolCatalog.exec.ts` is the entry
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

The sibling `octocode-core/src/toolContract/discovery/` modules own agent-facing
fields, variants, relations, examples, and input preparation. Interfaces import
these directly from `@octocodeai/octocode-core/schema`. Core also owns CLI and
MCP context text; tools-core supplies runtime availability and execution.
Public query envelopes are validated in full before runtime initialization.
Input preparation rejects unknown fields by default with a correction hint;
adapters must explicitly opt into field filtering. Valid batches isolate runtime
query failures and retain one indexed result per query; they are not transactions.

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
- Public input fields and refinements live in `@octocodeai/octocode-core/schema`.
  Output schemas are not published.
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
- `src/direct.ts` — the minimal `./direct` entry: `executeDirectTool` and response rendering.
- `src/schema.ts` — engine-free runtime availability and internal search workflow
  translation. All public schemas, discovery/presentation helpers, and input
  preparation come directly from `@octocodeai/octocode-core/schema`.
- `./platform`, `./session`, `./config`, `./credentials`,
  `./paths`, `./fs-utils`, `./testing` — focused subpath entries (see
  `package.json#exports`).
- Public entries export directly from their owning module. Internal consumers
  import that owner instead of routing through an unrelated module's exports.

## Distribution

`@octocodeai/octocode-tools-core` is a published runtime package with public
entry points in `package.json#exports`. Workspace consumers resolve its source
during development.

- `octocode-mcp` keeps tools-core external and declares it as a runtime
  dependency; npm installs it with the server.
- `octocode` also keeps tools-core external as a declared runtime dependency.
  Each package resolves its own dependency graph.
- Native dependencies, especially `@octocodeai/octocode-engine` and its matching
  platform package, remain external.

Publish runtime prerequisites before their consumers: engine platform packages,
the engine root, config/core/tools-core, and then the CLI and MCP interfaces.

## Rules

- Keep logic here, not in consumers — the CLI/MCP only select and render.
- Descriptions and schemas come from `@octocodeai/octocode-core/schema`; don't hardcode them in interfaces or runners.
- Native work (minify, search, graph-fact scanning, structural, LSP, masking) goes through
  `octocode-engine`, never reimplemented in TS.
- Add a public contract in core, then its `src/tools/<name>/` runner and runtime
  attachment in `toolConfig.ts`. Keep shared workflow rules in core's `/mcp` entry.
