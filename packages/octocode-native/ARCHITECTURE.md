# Native tool runtime architecture

`octocode-native` is the sole public tool execution owner.

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

The runtime executes all eleven tools:

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

Availability is resolved natively. GitHub and artifact tools are enabled by default; local tools honor local policy; cloning requires its feature gate and persistent storage. Contract preparation accepts direct, array, and `{ "queries": [...] }` forms, validates the complete bulk envelope, and preserves ordered row indexes and isolated domain failures.

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

`octocode-engine` is consumed as a Rust library with default features disabled. It exposes reusable algorithms, not public policy. N-API engine bindings remain available to engine consumers but are not an alternate Octocode tool runtime.

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

`@octocodeai/octocode-core` is the external contract authoring owner. Its generator emits `src/contracts/generated/` with the contract JSON, Rust constant, validation fixtures, provenance revision, and fingerprint. Tests reject stale, dirty, or fingerprint-mismatched provenance. Generated files are not hand edited.

## Build modes

- Binary builds use `--no-default-features` and contain the full CLI/runtime.
- Addon builds enable `napi-addon` and expose the same runtime to MCP.
- Platform packages contain optimized native CLI, regex-worker, and addon artifacts.
- Release acceptance exercises the direct native CLI, the built Node launcher, direct N-API calls, and real stdio MCP calls.
