# octocode-engine architecture

`octocode-engine` is the single Rust engine crate and published npm package for
Octocode. It owns all algorithm primitives (search, LSP, signatures, structural
analysis, minification, security, graph, index, text) plus the Node.js N-API
bindings that expose them to JavaScript. When built with the `napi-addon`
feature it produces the platform `.node` binary consumed by `octocode-tools-core`,
`octocode`, and `octocode-pi-extension`. `octocode-native` links this crate as a
pure `rlib` (no N-API) for the native CLI and MCP runtime.

## Boundary

- Extension filesystem mutations and edit-preview diff generation belong to
  `octocode-extension-rust`; this engine retains research-tool diff filtering.
- `src/lib.rs` owns all module declarations and explicitly re-exports the public
  N-API surface.
- `src/bindings/` is the FFI boundary. Keep wrappers thin: convert JS-owned
  values, call the owning domain module, and map errors once. No logic in bindings.
- Shared NAPI-safe Rust structs are defined once in `src/types.rs`.

## Domains

- `src/minify/`, `search/`, `text/`, `structural/`, `signatures/`, `graph/`,
  and `index/` own the reusable Rust implementations. Matching files under
  `src/bindings/` are N-API adapters only.
- `src/lsp/` owns the portable JSON-RPC client, grammar/config tables, command
  resolution, bounded notification transport, and shared pool lifecycle policy
  (canonical keys, in-flight deduplication, health checks, idle expiry, LRU
  eviction, and explicit cleanup).
- TypeScript under the package's `src/lsp/` owns Node-only command
  discovery/provisioning, workspace-root detection, URI/path validation, symbol
  resolution, and the compatibility manager API. Its pool wrapper delegates
  lifecycle ownership to the native facade. `octocode-tools-core` consumes this
  tier through the `./lsp/*` subpath exports.
- `src/security/` owns secret detection and sanitization. TypeScript wrappers
  retain Node orchestration such as `withSecurityValidation`, registries,
  validators, masking, and regex catalogs; these ship through the engine's
  `./security/*` subpath exports.
- Definition, type-definition, and implementation requests negotiate
  `LocationLink` support. Native conversion preserves the provider's symbol
  selection range separately from enclosing source context.

## Research graph direction

Reachability/dead-code detection is exposed by `astSearch`'s `topology`
operation, consuming this engine's per-file facts:

- `src/signatures/graph_facts.rs` (JS/TS via `js_oxc.rs`, other languages via
  Tree-sitter) extracts AST facts for declarations, imports, exports, calls,
  classes, and functions, normalized into common symbol/relation facts.
- `src/graph/mod.rs` owns the bounded filesystem walk, parallel file reads,
  native fact extraction, and conservative same-file reference counts behind the
  async `scanGraphFacts` binding. Rust consumers use typed facts; the JSON form
  is a compatibility adapter for N-API and TypeScript consumers. Per-file
  omissions remain in the stable `skipped` diagnostic envelope.
- `src/graph/model.rs` owns immutable graph snapshots, domain IDs, AST, and LSP
  evidence, completeness, diagnostics, and deterministic build receipts.
- `src/graph/algorithms.rs` owns reusable file-topology traversal, shortest-path,
  SCC, condensation, cycle, and transitive-edge algorithms. Interface packages
  decide policy and response shape; the engine doesn't assign dead-code verdicts.

LSP provides semantic evidence for cross-file identity, references, definitions,
implementations, callers, callees, and call hierarchy. Text/ripgrep is discovery
only; `astSearch` topology output is candidate-grade and must be confirmed with
`lspSearch` before a deletion claim. See [Code graph architecture](docs/CODE_GRAPH.md)
for the evidence, freshness, and evaluation contracts.

Structural prefilters derive necessary literals from parsed rules. Native file
scans report `scanTruncated` when an extra candidate exists beyond `maxFiles`;
result pagination can expand that scan.

Search-result AST classification shares a two-second cooperative deadline across
candidate files. Exhausted classification leaves remaining hits unlabeled; it
does not discard search results. Execution limits carry staged diagnostics and an
incomplete status. Public tools preserve these diagnostics and report terminal
limits when no continuation can complete the execution.

## Rules

- No logic in `lib.rs` or `src/bindings/`.
- New reusable Rust code goes in the closest domain module under `src/`.
- Stateful Node host orchestration (security registries, LSP provisioning)
  remains in the TypeScript tier.
- Declare all public N-API and Rust benchmark exports explicitly in `lib.rs`.
  Avoid wildcard relay exports and compatibility-only duplicate modules.
- Avoid duplicate helpers across domains. Shared LSP command/path checks live in
  `src/lsp/commands.rs`.

## Cargo deps

`package.json#version` is the release version source of truth.
`yarn version:sync` updates `Cargo.toml`, `Cargo.lock`, root
`optionalDependencies`, and every `npm/<platform>/package.json` to match it.

- N-API: `napi`, `napi-derive`; build: `napi-build`.
- Serialization/text: `serde`, `serde_json`, `serde_yaml_ng`, `regex`,
  `regex-syntax`, `aho-corasick`, `url`.
- Async/process/LSP: `tokio`, `which`.
- Search: `grep`, `ignore`, `memmap2`, `crossbeam-epoch`, `rayon`.
- Minify/JS/CSS: `lightningcss`, `oxc_allocator`, `oxc_ast`, `oxc_codegen`,
  `oxc_minifier`, `oxc_parser`, `oxc_span`, `oxc_semantic`.
- Structural search: `tree-sitter` + grammars for TypeScript, JavaScript,
  Python, Go, Rust, Java, C, C++, C#, Ruby, PHP, Kotlin, JSON, YAML, HTML,
  CSS, SCSS, Scala, SQL, Swift.

## Distribution

`@octocodeai/octocode-engine` ships as:

- a root package with JS/TS loader files and `dist/` wrappers, but no `.node`
  binary in the root tarball;
- six platform packages under `npm/<platform>/`, each containing exactly one
  `octocode-engine.<platform>.node` binary;
- exact root `optionalDependencies` pointing at those six platform packages.

The root loader supports ESM and CJS, detects platform/libc, then loads the
local dev binary, bundled standalone runtime binary, or matching npm optional
dependency.

Publish the six platform packages first, then publish the engine root. Interface
packages (`octocode-mcp` and `octocode`) are published only after this package
is on npm.

## Cross-compile build prerequisites

`yarn build:all` cross-compiles the native addon for all 6 target platforms:

| platform | extra prerequisites |
|---|---|
| `darwin-arm64` | host target — no extras |
| `darwin-x64` | `rustup target add x86_64-apple-darwin` |
| `linux-x64-gnu` | `rustup target add x86_64-unknown-linux-gnu` + `brew install zig` |
| `linux-x64-musl` | `rustup target add x86_64-unknown-linux-musl` + `brew install zig` |
| `linux-arm64-gnu` | `rustup target add aarch64-unknown-linux-gnu` + `brew install zig` |
| `win32-x64-msvc` | `rustup target add x86_64-pc-windows-msvc` + `brew install llvm` + export PATH |

## Verification

```bash
yarn version:sync
yarn build:all
yarn prepublish:verify
yarn verify:rust
yarn verify
```
