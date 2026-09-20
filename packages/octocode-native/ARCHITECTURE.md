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

Availability is resolved natively. GitHub and artifact tools are enabled by default; local tools honor local policy; cloning requires its feature gate and persistent storage. `jev` is available only when the resolved `OCTOCODE_JEV_KEY` is nonblank. Contract preparation accepts direct, array, and `{ "queries": [...] }` forms, validates the complete bulk envelope, and preserves ordered row indexes and isolated domain failures.

`jev` accepts caller-supplied `state`, typed `questions`, and optional named `sources`; configuration selects the model. `runtime/jev_sources` loads bounded local/GitHub files through existing path, content-security and provider policies. With sources, provider state wraps caller values under `context` and file bodies under `sources`; the response adds provenance/hash receipts without bodies. The tool generates no questions and makes no workflow decisions. Shared transport enforces deadlines, cancellation, bounded responses, and no redirects.

Source hydration scans at most 1 MiB per raw file before selecting at most 64 KiB per source and 256 KiB combined. Full-source redaction precedes range selection. Jev rejects response-pagination controls before source reads or inference; query replay cannot retrieve a page of a previous judgment. GitHub content-cache disk keys include the transport's credential/endpoint partition. Normal shutdown clears memory; explicit cache clearing purges disk.

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

`crates/engine/src/signatures/languages.rs` is the sole native grammar inventory. The default release registers ten first-class families and 25 extensions. Structural search/rewrite, signatures, graph facts, syntax inspection, and LSP grammar adapters derive from that registry; built-in server routes have exact-set tests against the same product boundary. YAML rule parsing is configuration syntax, not YAML source support.

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

`@octocodeai/octocode-core` is the external contract authoring owner. Its generator emits `crates/runtime/src/contracts/generated/` with the contract JSON, Rust constant, validation fixtures, provenance revision, and fingerprint. Tests reject stale, dirty, or fingerprint-mismatched provenance. Generated files are not hand edited.

## Build modes

- Binary builds use `--no-default-features` and contain the full CLI/runtime.
- `build:runtime:dev` stages the host binaries into their platform package so the local CLI launcher executes the rebuilt runtime.
- Addon builds enable `napi-addon` and expose the same runtime to MCP.
- Each platform package contains the optimized native CLI, regex worker, runtime addon, and engine addon.
- Root entrypoints are lazy and independent: `.`/`./runtime` load only the runtime addon, while `./engine` loads only the engine addon.
- Darwin addons are ad-hoc signed after staging because target-specific linker signatures are not a sufficient loadability guarantee.
- Host-platform staging and platform verification load both addons in subprocesses before release acceptance.
- Release acceptance exercises the direct native CLI, the built Node launcher, direct N-API calls, and real stdio MCP calls.
