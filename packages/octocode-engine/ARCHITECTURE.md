# Octocode engine architecture

`octocode-engine` is the published napi-rs package and TypeScript host adapter
for the reusable Rust implementation in `../octocode-engine-core`. Rust under
`src/bindings/` converts JS-owned values, calls core primitives, and preserves
the public NAPI surface. The TypeScript layer in `src/lsp/` and `src/security/`
owns Node-only discovery, provisioning, validation, registry, and wrapper
behavior; reusable algorithms, domain types, errors, security primitives, and
LSP lifecycle policy live in core. Core and binding Rust are tested with
`cargo test`; TypeScript wrappers with `vitest`.

## Boundary

- Extension filesystem mutations and edit-preview diff generation belong to
  `octocode-extension-rust`; this engine retains research-tool diff filtering.
- `src/lib.rs` wires bindings and explicitly re-exports the public NAPI surface.
- `src/bindings/` is the FFI boundary. Keep wrappers thin: convert JS-owned
  values, call `octocode-engine-core`, and map errors once.
- Shared NAPI-safe Rust structs are defined once in `../octocode-engine-core/src/types.rs`
  and exposed here through the existing facade.

## Domains

- `../octocode-engine-core/src/minify/`, `search/`, `text/`, `structural/`,
  `signatures/`, `graph/`, and `index/` own the reusable Rust implementations.
  Matching files under `src/bindings/` are NAPI adapters only.
- `../octocode-engine-core/src/lsp/` owns the portable JSON-RPC client,
  grammar/config tables, command resolution, bounded notification transport,
  and shared pool lifecycle policy (canonical keys, in-flight deduplication,
  health checks, idle expiry, LRU eviction, and explicit cleanup).
- TypeScript under `src/lsp/` owns Node-only command discovery/provisioning,
  workspace-root detection, URI/path validation, symbol resolution, and the
  compatibility manager API. Its pool wrapper delegates lifecycle ownership to
  the shared core-backed native facade. tools-core consumes this tier through
  the `./lsp/*` subpath exports.
- `../octocode-engine-core/src/security/` owns secret detection and sanitization.
  TypeScript wrappers under `src/security/` retain Node orchestration such as
  `withSecurityValidation`, registries, validators, masking, and regex catalogs;
  these continue to ship through the engine's `./security/*` subpath exports.
- Definition, type-definition, and implementation requests negotiate
  `LocationLink` support. Native conversion preserves the provider's symbol
  selection range separately from enclosing source context. Plain `Location`
  ranges remain unchanged.

## Research graph direction

Reachability/dead-code detection is exposed by `astSearch`'s `topology`
operation, consuming this engine's per-file facts rather than tool-specific
regex logic:

- `../octocode-engine-core/src/signatures/graph_facts.rs` (JS/TS via
  `js_oxc.rs`, other registered languages via Tree-sitter) parses files through
  the shared grammar registry and extracts
  AST facts for declarations, imports, exports, calls, classes, and functions,
  normalized into common symbol/relation facts;
- `../octocode-engine-core/src/graph/mod.rs` owns the bounded filesystem walk,
  parallel file reads, native
  fact extraction, and conservative same-file reference counts behind the
  async `scanGraphFacts` batch binding. The outer scan result and each embedded
  fact payload carry the same additive `schemaVersion`. Per-file omissions are
  reported in the stable `skipped` diagnostic envelope (`relativePath`, `code`,
  `message`) instead of being recoverable only from an aggregate count.
  Tools-core validates the fact version, preserves those diagnostics, and
  connects accepted facts into file/symbol/dependency graph nodes and edges;
  exported declaration names also travel through a crate-private extraction
  result so reference counting does not parse the just-serialized JSON;
- tools-core owns graph-policy algorithms over those facts:
  `../octocode-tools-core/src/graph/reachability.ts` runs BFS reachability and
  iterative Tarjan's SCC, while
  `../octocode-tools-core/src/tools/ast_search/topology/deadCodeScan.ts`
  performs transitive-dead pruning. The engine does not assign dead-code
  verdicts.

LSP remains the semantic proof layer for cross-file identity, references,
definitions, implementations, callers, callees, and call hierarchy. Text/ripgrep
is discovery only; `astSearch` topology output is candidate-grade and must be
confirmed with `lspSearch` before a deletion claim, matching that rule.

Graph declaration IDs identify occurrences using scope and source position.
Same-named methods and separate Rust declaration/implementation occurrences
remain distinct. An unresolved call spelling is a reference candidate, not a
resolved same-file symbol. The public graph reports linking coverage separately
from parser support: declared Rust modules, literal `#[path]` modules, and JS/TS
relative imports have linkers. Tools-core can opt into bounded, offline Cargo
metadata to identify custom crate roots, editions, and workspace dependency aliases.
Conditional compilation and macro expansion remain unsupported; unresolved internal imports and parse recovery
produce explicit coverage diagnostics. Same-file lexical occurrence counts are
conservative retention evidence, not semantic references.

Structural prefilters derive necessary literals from parsed rules. Negation
does not supply a positive anchor, and an unrestricted OR branch disables a
restrictive prefilter. Native file scans report `scanTruncated` when an extra
candidate exists beyond `maxFiles`; result pagination can expand that scan.
The asynchronous scan result carries the query plan used for that same scan,
so callers can explain zero matches without repeating a synchronous directory
walk on the JavaScript event loop.

Search-result AST classification shares a two-second cooperative deadline across
the candidate files. Exhausted classification leaves remaining hits unlabeled;
it does not discard search results. Signature queries cache compiled queries,
honor execution limits, and remove only nodes explicitly captured as `@body`.
Helper captures used by predicates cannot remove signature lines.
Execution limits instead carry staged diagnostics and an incomplete status.
Completed files remain available, while exhausted matching cannot establish
absence or satisfy a negation. Public tools preserve these diagnostics and
report terminal limits when no continuation can complete the execution.

**Note:** core's `signatures/graph_facts.rs`/`extractGraphFacts` has live consumers
through both the single-file API and `scanGraphFacts` — it is not orphaned. A
native Rust port of the graph algorithms above
(reachability/SCC/retainer-lookup/pruning) was
scoped in `docs/NATIVE_GRAPH_DOMAIN_SCOPE.md` but is superseded by this
TypeScript implementation; see that doc's status before reviving the idea.

## Rules

- Do not put logic in `lib.rs` or `bindings/`.
- Put new code in the closest domain module; create a submodule only when a file
  gains a separate responsibility.
- Reusable pure Rust modules belong in `octocode-engine-core`; NAPI conversion
  belongs at this package's binding edge.
- Stateful Node host orchestration such as security registries remains in the
  TypeScript tier. LSP process lifecycle policy and native protocol state live
  in core and are bounded before TypeScript can query them.
- Declare the public NAPI and Rust benchmark exports explicitly in `lib.rs`.
  Internal callers import from core or the owning binding; avoid wildcard relay
  exports and compatibility-only duplicate modules.
- Avoid duplicate helpers across domains. Shared LSP command/path checks live in
  `../octocode-engine-core/src/lsp/commands.rs`.

## Cargo Deps

`package.json#version` is the release version source of truth for the engine.
`yarn version:sync` updates `Cargo.toml`, `Cargo.lock`, the root
`optionalDependencies`, and every `npm/<platform>/package.json` to match it.

- NAPI: `napi`, `napi-derive`; build: `napi-build`; dev: `napi`.
- Serialization/text: `serde`, `serde_json`, `serde_yaml_ng`, `regex`,
  `regex-syntax`, `aho-corasick`, and `url`.
- Async/process/LSP: `tokio`, `which`.
- Search: `grep`, `ignore`; patched transitive security floors are pinned for
  `crossbeam-epoch` and `memmap2`.
- Minify/JS/CSS: `lightningcss`, `oxc_allocator`, `oxc_ast`, `oxc_codegen`,
  `oxc_minifier`, `oxc_parser`, `oxc_span`, `oxc_semantic`.
- Structural search: `tree-sitter`.
- Grammars: `tree-sitter-typescript`, `tree-sitter-javascript`,
  `tree-sitter-python`, `tree-sitter-go`, `tree-sitter-rust`,
  `tree-sitter-java`, `tree-sitter-c`, `tree-sitter-cpp`,
  `tree-sitter-c-sharp`, `tree-sitter-ruby`, `tree-sitter-php`,
  `tree-sitter-kotlin-ng`, `tree-sitter-json`,
  `tree-sitter-yaml`, `tree-sitter-html`,
  `tree-sitter-css`, `tree-sitter-scss`, `tree-sitter-scala`,
  `tree-sitter-sequel`, and
  `tree-sitter-swift`.

## Distribution

`@octocodeai/octocode-engine` is the only published native package in this
repo. It ships as:

- a root package with JS/TS loader files and `dist/` wrappers, but no `.node`
  binary in the root tarball;
- six platform packages under `npm/<platform>/`, each containing exactly one
  `octocode-engine.<platform>.node` binary;
- exact root `optionalDependencies` pointing at those six platform packages.

The root loader supports both ESM and CJS entrypoints, detects the current
platform/libc, then loads the local dev binary, bundled standalone runtime
binary, or matching npm optional dependency.

Publish the six platform packages first, then publish the engine root. Interface
packages (`octocode-mcp` and `octocode`) are published only after this package is
available on npm because they depend on it directly at runtime.

## Cross-compile build prerequisites

`yarn build:all` cross-compiles the native addon for all 6 target platforms.
The default `rustup` install only ships the host target; the others require:

| platform | extra prerequisites |
|---|---|
| `darwin-arm64` | host target — no extras |
| `darwin-x64` | `rustup target add x86_64-apple-darwin` |
| `linux-x64-gnu` | `rustup target add x86_64-unknown-linux-gnu` + `brew install zig` |
| `linux-x64-musl` | `rustup target add x86_64-unknown-linux-musl` + `brew install zig` |
| `linux-arm64-gnu` | `rustup target add aarch64-unknown-linux-gnu` + `brew install zig` |
| `win32-x64-msvc` | `rustup target add x86_64-pc-windows-msvc` + `brew install llvm` + export PATH |

One-time setup on macOS (covers all 6 platforms):

```bash
# Rust cross targets
rustup target add x86_64-apple-darwin \
  x86_64-unknown-linux-gnu x86_64-unknown-linux-musl \
  aarch64-unknown-linux-gnu x86_64-pc-windows-msvc

# zig — used by napi-rs cargo-zigbuild for linux cross-linking
brew install zig

# LLVM — provides llvm-lib (MSVC archiver) for the win32 cross-build
brew install llvm
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"

# Now build all 6 platforms (~2 min each; ~12 min total)
yarn build:all
```

**Why zig?** `napi build --cross-compile` uses `cargo-zigbuild` under the hood.
`cargo-zigbuild` requires a `zig` binary on PATH; it does **not** auto-download
one. Without `zig`: `Error: Failed to find zig / cannot find binary path`.

**Why llvm?** The `cc-rs` build script of a C dependency (`pcre2`) needs
`llvm-lib` (LLVM’s MSVC-compatible archiver) when cross-compiling to
`x86_64-pc-windows-msvc`. `brew install llvm` installs it at
`/opt/homebrew/opt/llvm/bin/llvm-lib`. Without it:
`error occurred in cc-rs: failed to find tool "llvm-lib"`.

CI builds all 6 platforms in a zig-equipped Linux environment and publishes
them before the root package. See `RELEASE_GUIDE.md`.

## Verification

Run from `packages/octocode-engine/`:

```bash
yarn version:sync
yarn build:all
yarn prepublish:verify
yarn verify:rust
yarn verify
```
