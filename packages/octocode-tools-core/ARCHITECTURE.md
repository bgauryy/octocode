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

Exact LSP positions bypass fuzzy name resolution. Native import token ranges
identify alias candidates; tools-core verifies identity through matching
language-server definitions and collects their references before one canonical
deduplication and pagination pass. Inspection limits and failed verification
remain explicit partial coverage. Parser coordinates never replace semantic
evidence, and provider-scoped results do not prove exhaustive workspace usage.

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

### Lexical continuation snapshots

`src/tools/local_ripgrep/pageManifest.ts` owns optional immutable result manifests.
Explicit `noIgnore:true` searches can return an opaque `pagination.snapshot` and
carry it on executable file/match continuations. Default ignore-aware searches
remain live: their ancestor/global ignore dependencies are not exposed by the
native contract. Access-time sorts and scopes containing symlinks, unreadable
entries, inventory caps, or native search errors also remain live. Live pagination
carries a result fingerprint and rescans normally; a changed result/order/coverage
returns a restart before mixing pages. Volatile scan timings and byte counts are
excluded from this fingerprint.

Eligible searches validate two complete native filesystem inventories before
searching and again before saving sanitized results. Continuations recheck the
full scope, including previously unmatched files, using nanosecond file metadata.
This avoids repeating native content scans at the cost of filesystem metadata
work; it is not an unconditional latency improvement. Single-page eligible calls
perform the initial check but do not persist a manifest. Storage is private,
content-addressed, limited to 1 MiB per manifest, and pruned toward 64 entries
when writing. Manifests expire after 60 seconds. A supplied token that is expired,
evicted, changed, corrupt, or unsafe fails closed with `staleSnapshot` and a
schema-valid `next.restart`; it never silently rescans or serves stale snippets.

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

Handlers distinguish failed execution from incomplete evidence. `status: 'error'`
and domain receipts describe validation, provider, or recovery failures. Emit
`complete:false` / `isPartial:true` only for incomplete result evidence, with an
executable continuation or an explicit terminal limit. Shared response diagnostics
check those flags even on error rows; handlers must not attach them to every failure.

## Providers

GitHub-only today, behind an `ICodeHostProvider` abstraction so the surface stays
provider-agnostic. `src/providers/factory.ts` caches provider instances
(TTL + LRU, keyed by type/baseUrl/token hash). `src/tools/providerExecution.ts`
builds the execution context from `serverConfig`, runs operations, and
normalizes provider errors. GitHub API plumbing (client, search, content, PRs,
structure, history) lives in `src/github/`.

Issue listing keeps GitHub's repository-list endpoint, including its ordering and
coverage beyond the Search API window. Because that endpoint also returns PRs,
`issues/fetchers.ts` can skip up to five consecutive PR-only pages after the
requested page. It stops at the first issue-bearing page, provider exhaustion,
or that request bound. Pagination records the actual provider page and extra
requests; continuations resume after that page. This reduces empty agent calls,
not upstream API calls.

## Cross-cutting modules

- `src/cacheMaintenance.ts` — shared 24-hour maintenance gate, persisted marker,
  cross-process lock, owned-root sweep, and MCP deadline scheduler for
  `tmp/clone`, `tmp/tree`, and `tmp/response`.
- Public input fields and refinements live in `@octocodeai/octocode-core/schema`.
  Output schemas are not published.
- `src/utils/pagination/` (incl. `hints.ts` — next-step hints: pagination
  cursors, token-budget warnings, structure hints) + `src/utils/response/` — the
  shared YAML/JSON result rendering and lossless whole-response text pagination.
  Text pages preserve Unicode code points and carry executable snapshot-bound
  continuations. An offset inside a code point returns a restart. This pagination
  scopes `content.text`; it does not truncate `structuredContent`.
- Direct structured projections carry an internal per-execution render flag to
  skip unused text rendering after response normalization. Text still renders
  for pagination and errors; structured-content sanitization remains mandatory
  before egress.
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
