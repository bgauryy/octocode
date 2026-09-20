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
- `jev`

Availability is resolved natively. GitHub and artifact tools are enabled by default; local tools honor local policy; cloning requires its feature gate and persistent storage. `jev` is available only when the resolved `OCTOCODE_JEV_KEY` is nonblank. Contract preparation accepts direct, array, `{ "queries": [...] }`, and Jev matrix forms, validates the complete envelope, and preserves ordered row indexes and isolated domain failures.

`jev` accepts independent `{reasoning,context,question}` pairs or a resource-question matrix. Native contract preparation validates matrix IDs and the 25-cell cap, trims root reasoning, then expands resource-major rows with `resourceId` and `questionId`. The IDs are correlation metadata and never provider evidence. Configuration selects the model. `runtime/domain_dispatch` is the shared execution path for ordinary tools and hidden context; `runtime/jev_context` validates the nested canonical query, enforces availability and security, sanitizes the ordinary result and checks its output contract before inference. It never re-enters public request admission.

The provider receives the supplied inline value or sanitized ordinary result envelope. `runtime/jev_batch` keeps flat captures independent, but reuses one exact `(resourceId,context)` capture across matrix questions, then groups structurally identical states under byte headroom. `tools/jev` adapts singletons or bounded groups and projects one typed answer per row. Independent provider groups run concurrently, bounded at five requests and sharing deadline/cancellation; context capture stays serial. Answers retain order and isolated errors. Tool context adds body-free hashes, bounded/partial coverage, and safe continuations or limitations. Partial results do not trigger automatic source-page reads.

Nested context supports the nine read tools; recursive Jev, astRewrite and ghCloneRepo are rejected. Ordinary budgets remain active and provider requests are capped at 4 MiB. Grouping uses conservative 24 KiB state-plus-question and 48 KiB combined headroom, splitting groups automatically; a single larger state keeps singleton execution. These byte budgets are not token estimates. Jev rejects response pagination; callers bound huge resources before inference and explicitly follow source continuations. Flat repeated retrieval stays fresh, while matrices reuse captures. There is no judgment cache. Group usage is recorded once; `usageAttribution` assigns totals to the first successful row and zero allocated tokens to siblings. Failed-only groups may lack public usage; absence is not zero consumption.

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
