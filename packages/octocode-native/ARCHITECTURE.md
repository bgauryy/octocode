# Native tool runtime architecture

`octocode-native` is the sole public tool execution owner and the npm distribution owner for both the runtime and engine primitive addons.

```text
native CLI ───────────────────┐
                             ├──▶ Rust ToolRuntime
Node MCP → N-API adapter ─────┘       ├── generated octocode-core contracts
                                      ├── config and security policy
                                      ├── providers, credentials, and caches
                                      ├── tool orchestration and responses
                                      └── octocode-engine primitives
```

The CLI never loads N-API or JavaScript. The MCP addon and native CLI call the same Rust runtime. JavaScript interfaces own protocol framing, registration, interactive selection, and process startup only.

## Public catalog

The runtime executes all twelve tools:

- `ghSearch`
- `ghGetFileContent`
- `ghSearchHistory`
- `ghGetHistoryItem`
- `ghCloneRepo`
- `artifactSearch`
- `localSearch`
- `localFetch`
- `astSearch`
- `astRewrite`
- `lspSearch`
- `clasify`

Availability is resolved natively. GitHub and artifact tools are enabled by default; local tools honor local policy; cloning requires its feature gate and persistent storage. `astRewrite` has a separate default-off `ENABLE_AST_REWRITE` availability gate, while apply retains `ENABLE_AST_REWRITE_APPLY`. `clasify` is available to MCP only when the resolved `OCTOCODE_CLASSIFICATION_API` is nonblank; the CLI command returns an actionable missing-key error when invoked without it. Ordinary contract preparation accepts direct, array, and `{ "queries": [...] }` forms. Semantic assessment accepts one complete `SemanticQuery` directly or a batch of complete queries, validates the full resource-question matrices, and preserves isolated page failures.

`clasify` applies every `questions[]` entry to every `resources[]` entry. Native contract preparation validates IDs, the 25-cell per-query cap, and the 50-cell batch cap, then expands resource-major work with `queryId`, `resourceId`, and `questionId`. The IDs are correlation metadata and never provider evidence. Questions are native Noul, Choice, or Score primitives: instructions are non-null/non-empty structured values; Choice and Noul descriptions may be null at their documented boundaries; Score levels may not. Configuration selects the Jev provider model. `runtime/domain_dispatch` is the shared execution path for ordinary tools and hidden context; `runtime/jev_context` validates the nested canonical query, enforces availability and security, sanitizes the ordinary result and checks its output contract before inference. It never re-enters public request admission.

The provider receives the supplied non-empty inline value or sanitized ordinary result envelope. `runtime/jev_batch` captures each logical resource once, reuses it across its questions, and groups structurally identical states under byte headroom. `tools/jev` remains the private provider adapter; it handles singleton or bounded grouped calls and projects one typed answer per cell. Provider groups are dispatched sequentially under one shared deadline/cancellation budget; grouping coalesces same-state questions into single requests rather than issuing concurrent calls. Answers retain order and isolated errors. Same-resource source/provider pages are emitted automatically as ordered `pages[]` entries and are never silently averaged or reduced. Successful pages preserve `requestedModel` and `resolvedModel` separately; context adds body-free hashes, coverage, continuations, and limitations.

Nested context supports the nine read tools; recursive semantic assessment, astRewrite and ghCloneRepo are rejected. Ordinary budgets remain active and provider requests are capped at 4 MiB. Grouping uses conservative byte headroom and splits groups automatically; those byte budgets are not token estimates. If work remains beyond the current bounded response, the query returns executable `next.clasify`; callers run it unchanged and append its pages. There is no judgment cache. Group usage is recorded once; failed-only groups may lack public usage, and absence is not zero consumption.

## Ownership

| Module | Owns | Must not own |
|---|---|---|
| `config` | Native configuration acquisition and diagnostics | Tool behavior |
| `contracts` | Generated schemas/rules, defaults, validation, and canonical contract fingerprint | Provider or tool execution |
| `runtime` | Admission, dispatch, request context, cancellation, ordered bulk orchestration, and exit classification | CLI presentation or MCP SDK types |
| `policy` / `security` | Path, content, command, and secret policy | Interface-specific behavior |
| `providers` / `registries` / `cache` | Remote DTOs, HTTP, retries, credentials, endpoint caches | CLI or MCP framing |
| `tools` | Public operations composed from shared runtime services and engine primitives | Independent config, auth, or response systems |
| `response` | Contract-checked rows, compression, pagination, continuations, and sanitized rendering | Tool execution |
| `lsp` | Runtime composition around the engine language-server pool | A second lifecycle implementation |
| `adapter_napi` | Host conversion and native runtime lifecycle | An alternate execution path |
| `cli` | Arguments, human output, and shell exits | Node, N-API, or duplicated tools |

`crates/engine` is consumed as a Rust library with default features disabled. It exposes reusable algorithms, not public policy. Its N-API bindings are published at `@octocodeai/octocode-native/engine` but are not an alternate Octocode tool runtime.

## Language capability ownership

`crates/engine/src/signatures/languages.rs` is the sole native grammar inventory. The default release registers 12 first-class families and 30 extensions. Structural search/rewrite, signatures, graph facts, syntax inspection, directory language filters, and LSP grammar adapters derive from that registry. Built-in semantic-server routing is a narrower, separately tested capability: 11 families and 27 extensions because generic Assembly requires trusted custom server configuration, while CUDA routes to `clangd`. YAML rule parsing is configuration syntax, not YAML source support.

Generic text search, reads, minification, file recognition, GitHub/history operations, artifact registries, and trusted custom LSP configuration do not consult the grammar allowlist. Syntax graph relations remain candidate evidence; callers use explicit LSP operations for semantic proof.

## Safety and lifecycle invariants

- Native runtime absence fails closed at every Node interface.
- Requests are admitted before asynchronous work starts and own cleanup through completion.
- Cancellation, worker handles, HTTP clients, caches, and LSP clients are runtime-owned resources.
- Credentials are pinned per request and never copied into public responses.
- Every completed row is checked against its generated tool output contract.
- Pagination and continuations retain the query identity needed to resume safely.
- Structural rewrite uses embedded engine primitives while the tool layer retains locks, hashes, path policy, selection, postconditions, transactions, and recovery.
- Public rewrite paths are relative to the preview root; guarded apply resolves them against that root.
- No external search, AST, rewrite, or provider executable is used. Intentional subprocesses are limited to system Git cloning, configured language servers, the bounded regex worker, and supported credential discovery.

## Contract generation

`@octocodeai/octocode-core` is the external tool-contract authoring owner. Its generator emits `crates/runtime/src/contracts/generated/` with the tool contract JSON, Rust constant, validation fixtures, provenance revision, and fingerprint. Tests reject stale, dirty, or fingerprint-mismatched provenance.

Configuration policy is independently owned by `@octocodeai/config` in `config-contract.json`. Native `build.rs` validates that declaration against its meta-schema and emits Rust config structs, defaults, environment policy, and generic resolver/validator metadata into `OUT_DIR`. Native-only builds therefore validate the full config contract without a prior Node generation step. Generated files are not hand edited.

## Build modes

- Binary builds use `--no-default-features` and contain the full CLI/runtime.
- `build:runtime:dev` stages the host binaries into their platform package so the local CLI launcher executes the rebuilt runtime. Staging atomically replaces each executable inode to avoid stale macOS code-signature caching after an in-place overwrite.
- Addon builds enable `napi-addon` and expose the same runtime to MCP.
- Each platform package contains the optimized native CLI, regex worker, runtime addon, and engine addon.
- Root entrypoints are lazy and independent: `.`/`./runtime` load only the runtime addon, while `./engine` loads only the engine addon.
- Darwin addons are ad-hoc signed after staging because target-specific linker signatures are not a sufficient loadability guarantee.
- Host-platform staging and platform verification load both addons in subprocesses before release acceptance.
- Release acceptance exercises the direct native CLI, the built Node launcher, direct N-API calls, and real stdio MCP calls.
