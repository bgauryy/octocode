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

The runtime executes all thirteen tools:

- `ghSearch`
- `ghGetFileContent`
- `ghSearchHistory`
- `ghGetHistoryItem`
- `ghCloneRepo`
- `artifactSearch`
- `localSearch`
- `localFetch`
- `astSearch`
- `astTopology`
- `astRewrite`
- `lspSearch`
- `clasify`

Availability is resolved natively. GitHub and artifact tools are enabled by default; local tools honor local policy. `ghCloneRepo` is available only through the CLI and requires persistent storage; MCP cannot execute it. `astRewrite` and `astTopology` are gated solely by `OCTOCODE_BETA` (env or `local.beta`) and default off; for `astRewrite`, the gate permits both preview and hash-guarded apply. `clasify` is available to MCP only when the resolved `OCTOCODE_CLASSIFICATION_API` is nonblank; the CLI command returns an actionable missing-key error when invoked without it. Ordinary contract preparation accepts direct, array, and `{ "queries": [...] }` forms. Semantic assessment accepts one complete `SemanticQuery` directly or a batch of complete queries, validates the full resource-question matrices, and preserves isolated page failures.

`clasify` applies every `questions[]` entry to every `resources[]` entry. Native contract preparation validates IDs, the 25-cell per-query cap, and the 50-cell batch cap, then expands resource-major work with `queryId`, `resourceId`, and `questionId`. The IDs are correlation metadata and never provider evidence. Explicit research presets expand once per matrix from core-owned templates into one Noul each; custom questions remain unchanged. Provider questions are Noul, Choice, or Score primitives: instructions are non-null/non-empty structured values; Choice and Noul descriptions may be null at their documented boundaries; Score levels may not. Configuration selects the classification provider model. `runtime/domain_dispatch` is the shared execution path for ordinary tools and hidden context; `runtime/clasify_context` validates the nested canonical query, enforces availability and security, sanitizes the ordinary result and checks its output contract before inference. It never re-enters public request admission. Independent read-only rows in one ordinary batch—including GitHub API searches and local searches—execute concurrently while output rows retain input order. Mutating clone/rewrite rows remain ordered, and `clasify` uses its dedicated scheduler.

The provider receives the supplied non-empty inline value, or the sanitized ordinary result's row data with the shared path `base`; the response envelope and the `next`, `diagnostics`, and `hints` control fields are withheld and excluded from the `maxChars` budget. `runtime/clasify_batch` captures each logical resource once, reuses it across its questions, and groups questions over each captured state under byte headroom. Delegated reads are limited to four per tool call across its matrices and sixteen per process. `tools/clasify` remains the private provider adapter; it handles singleton or bounded grouped calls and projects one typed answer per cell. Independent matrices and resource pages execute concurrently through a process-wide endpoint gate configured by `classification.maxConcurrency`, under one deadline/cancellation budget; grouping still coalesces same-state questions into single requests. Answers and matrices retain input order with isolated errors. Only document reads (`localFetch`, `ghGetFileContent`, `ghGetHistoryItem`) follow same-resource continuations automatically; search and discovery resources capture the requested page and return the rest through `next.clasify`. Pages are emitted as ordered `pages[]` entries and are never silently averaged or reduced; a followed page's receipt drops its consumed continuation. Internal answers preserve requested and resolved model identities; public output reports the first successful resolved model once per query, alongside summed usage. Mixed resolved versions are not represented separately. Context adds body-free hashes, coverage, continuations, and limitations.

Nested context supports ten read tools; recursive semantic assessment, astRewrite, ghCloneRepo, and ghSearch tree materialization are rejected. Ordinary budgets remain active and provider requests are capped at 4 MiB. Grouping uses conservative byte headroom and falls back to individual questions when the full group does not fit; those byte budgets are not token estimates. If work remains beyond the current bounded response, the query returns executable `next.clasify`; callers run it unchanged and append its pages. There is no judgment cache. Group usage is recorded once; failed-only groups may lack public usage, and absence is not zero consumption.

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
| `crates/runtime-napi` | Host conversion and native runtime lifecycle | An alternate execution path |
| `crates/cli` | Arguments, human output, and shell exits | Node, N-API, or duplicated tools |

`crates/runtime` is a pure `rlib`. `crates/github` owns GitHub protocol and
transport services; runtime keeps configuration, credential selection, cache policy,
and generated tool contracts. `crates/cli` owns `octocode` and
`octocode-regex-worker`; `crates/runtime-napi` owns the runtime `cdylib` and its
Node conversions. All five crates inherit one workspace version and remain
unpublished Cargo packages. `check:crate-boundaries` validates these boundaries.

`crates/engine` is consumed as a Rust library with default features disabled. It exposes reusable algorithms, not public policy. Its N-API bindings are published at `@octocodeai/octocode-native/engine` but are not an alternate Octocode tool runtime.

## GitHub authentication

`providers/github/auth` owns one lazy credential-selection flow shared by tool
requests and CLI auth inspection. Config owns environment-token precedence and
protected keys through `config-contract.json`; Node interfaces pass their
environment through without discovering or injecting credentials.

The resolver checks an explicit override, then the configured host's environment
token, encrypted home credentials, existing OS credentials, and finally host-scoped
`gh auth token`. Stored
credentials retain their username and expiry metadata. Request mode refreshes
expired stored credentials under the existing cross-process lock and rereads
storage after acquiring it; inspection mode never refreshes. A deleted credential
is not reused. A usable `gh` fallback keeps its own source identity and never
enters the stored-token refresh path. CLI source labels remain `env`, `platform`,
`gh-cli`, and `none`; home credentials add `octocode-storage`.

Discovery uses the runtime's explicit environment and adds common Homebrew paths
after the supplied PATH. Environment tokens are removed from the `gh` subprocess
because the resolver has already applied their host scope. Its output is bounded,
and timeout/cancellation paths kill and reap the child. Discovery and refresh
share the request deadline; cancellation and timeout do not start another fallback.
OS keychain calls run on blocking workers and are awaited before checking the
budget again: an OS keychain prompt cannot be forcibly cancelled by the resolver.
The selected credential is pinned before transport/cache work begins. Local
tools and catalog inspection perform no credential discovery.

`CredentialStore` takes the resolved Octocode home from config. New logins and
N-API credential saves use main-compatible AES-256-GCM home files; existing native
OS-store credentials remain a fallback. Refresh persists to its original source.
Home updates lock the document and atomically replace it, preserving other hosts.
A refresh cannot overwrite a newer home login or recreate a deleted credential.
Forced login replaces credentials only after successful authorization and saving.
Logout removes the host from both stores and leaves environment and gh auth alone.

Keep discovery and OAuth in native: both the standalone binary and N-API runtime
need the same lazy, host-scoped selection, refresh locks, deadlines, and provenance.
Moving those effects into Node config would duplicate the standalone path and
flatten credentials into environment tokens, losing refresh metadata and source.
Config remains the shared home/dotenv policy owner; core contracts are unchanged.

## Language capability ownership

`crates/engine/src/signatures/languages.rs` is the sole native grammar inventory. The default release registers 11 first-class families and 28 extensions. Structural search/rewrite, signatures, graph facts, syntax inspection, directory language filters, and LSP grammar adapters derive from that registry. CUDA is an optional grammar (`tree-sitter-cuda`), excluded from the default build because its parse tables cost ~6.8 MiB; only its native tree-sitter capabilities are gated off. Built-in semantic-server routing is a narrower, separately tested capability derived from an independent server table: 11 families and 27 extensions because generic Assembly requires trusted custom server configuration, while CUDA still routes `.cu`/`.cuh` to `clangd`. YAML rule parsing is configuration syntax, not YAML source support.

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

`@octocodeai/octocode-core` authors the tool contracts (Zod). `@octocodeai/config` is the only generator: `yarn contracts:regen` (repo root) refreshes core and writes `packages/octocode-config/contract/` — the enforcement contract JSON, parity fixtures, provenance, and generated Rust wire types. Native keeps no copy. `build.rs` embeds those files in place (`contracts::generated`), reruns when they change, and refuses to build when provenance or `tool_types.rs` names a different fingerprint than the contract. A test recomputes the embedded bytes' SHA-256 against provenance, so a hand edit fails without any native-side pin to update.

Tool wire types come from the same run: `contracts::tool_types` `include!`s `contract/tool_types.rs` (typify, from the bundled JSON Schema that also produces the TypeScript types). Ordinary tools parse their validated rows directly into generated `<Tool>Query` types (meta fields included) and build continuations from them. Tool modules may add accessor `impl` blocks for engine integer types, but never a second serde wire type. Clasify matrix orchestration still uses validated JSON values and defensive manual checks; migrating it to the generated query type remains an exception to remove. `tests/contract_field_effects.rs` fails only when the contract gains a field or discriminator value with no declared native effect in `field-effect-coverage.json`.

Configuration policy is independently owned by `@octocodeai/config` in `config-contract.json`. Native `build.rs` validates that declaration against its meta-schema and emits Rust config structs, defaults, environment policy, and generic resolver/validator metadata into `OUT_DIR`. Native-only builds therefore validate the full config contract without a prior Node generation step. Generated files are not hand edited.

## Build modes

`scripts/build-native.cjs` selects `octocode-cli` and `octocode-runtime-napi` in
one Cargo invocation, so shared dependencies resolve with one feature set for the
host artifacts. The engine addon resolves different engine features, so it builds
concurrently in its own target dir. Measured on a 12-core M-series, a debug
rebuild after an engine edit went from 51–56s (serial, one dir) to 41s.

- Binary builds select `octocode-cli` with `--no-default-features` and contain the full CLI/runtime.
- Every build stages the host binaries into their platform package so the local CLI launcher executes the rebuilt runtime. Staging (binaries and addons) atomically replaces each inode — never an in-place overwrite, which can crash a running MCP server that has the addon mapped or leave a stale macOS code-signature cache.
- Runtime addon builds select `octocode-runtime-napi` and expose the same runtime to MCP. Its optional `napi-test` feature is for Rust tests only. Engine addon builds keep `portable-default,napi-addon`.
- Each platform package contains the optimized native CLI, regex worker, runtime addon, and engine addon.
- Root entrypoints are lazy and independent: `.`/`./runtime` load only the runtime addon, while `./engine` loads only the engine addon.
- Darwin addons are ad-hoc signed after staging because target-specific linker signatures are not a sufficient loadability guarantee.
- Host-platform staging and platform verification load both addons in subprocesses before release acceptance.
- Release acceptance exercises the direct native CLI, the built Node launcher, direct N-API calls, and real stdio MCP calls.

Rust crate extraction does not create new npm packages. Build commands use the
committed lockfile; release preparation synchronizes versions before compilation,
and prepublish checks versions without rewriting manifests or resolving dependencies.
