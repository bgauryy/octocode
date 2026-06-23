# Octocode Engine Architecture

`octocode-engine` is a napi-rs native package **plus a TypeScript orchestration
layer**. Rust modules (reached through thin NAPI bindings in `src/bindings/`)
own the pure primitives — minify, search, structural, signatures, binary, text,
and the secret-detection/sanitizer core. The TS layer in `src/lsp/` and
`src/security/` owns what Rust cannot hold across NAPI calls — the LSP client
pool, symbol resolver, path/command validators, the security registry, and the
secret regex catalog. Rust is tested with `cargo test`; the TS wrappers with
`vitest`.

## Boundary

- `src/lib.rs` wires modules and re-exports the public NAPI surface.
- `src/bindings/` is the FFI boundary. Keep wrappers thin: convert JS-owned
  values, call inner Rust modules, map errors once.
- `src/types.rs` holds NAPI-safe shared structs.

## Domains

- `src/minify/` owns content minification: dispatch, comment removal, file-type
  config, and strategy implementations.
- `src/search/` owns local search: filesystem queries, matching-line extraction,
  ripgrep parsing, pattern validation, and in-process ripgrep search.
- `src/text/` owns small text utilities: diff filtering, extensions, UTF-8/UTF-16
  offsets, and YAML serialization.
- `src/structural/` owns Octocode AST search: language adapter, query
  validation, matcher compilation, file traversal, ripgrep-backed prefiltering,
  and result types.
- `src/lsp/` owns LSP support across two tiers: Rust (`*.rs`) — the NAPI
  `NativeLspClient` (JSON-RPC, lifecycle, symbol-kind, grammar/config tables);
  TypeScript (`*.ts`) — the client pool (`lspClientPool.ts`), manager
  (`manager.ts`), symbol resolver, URI/path validation, and workspace-root
  detection. tools-core consumes the TS tier through the `./lsp/*` subpath
  exports.
  `config.rs` also owns language-server command resolution: environment
  overrides first, then known fast paths such as `tsgo`, then package-local
  fallbacks such as `node_modules/typescript-language-server/lib/cli.mjs`.
  Resolver tests must inject cwd/PATH availability through helpers instead of
  mutating process-global cwd.
- `src/signatures/` owns semantic outlines and JS/TS symbol extraction.
- `src/security/` owns secret detection and sanitization across two tiers: Rust
  (`detector.rs`, `sanitizer.rs`, `patterns.rs`) for the detection engine and
  TypeScript wrappers (`withSecurityValidation`, `registry`, `pathValidator`,
  `commandValidator`, `mask`, `regexes/`) for orchestration. Both ship under
  the engine package via the `./security/*` subpath exports.

## Research Graph Direction

Future reachability/dead-code work belongs in a generic native graph domain, not
in tool-specific regex logic. The engine should provide language-neutral facts
and deterministic graph algorithms:

- parse files through the shared grammar registry;
- extract AST facts for declarations, imports, exports, calls, classes, and
  functions;
- normalize language-specific syntax into common symbol/relation facts;
- connect facts into file/symbol/dependency graph nodes and edges;
- run reachability, retainer lookup, strongly connected components, and
  transitive-dead pruning.

LSP remains the semantic proof layer for cross-file identity, references,
definitions, implementations, callers, callees, and call hierarchy. Text/ripgrep
is discovery only and must not produce deletion-grade proof. Framework/package
entrypoint policy and agent-facing packets stay in `octocode-tools-core` / OQL.

## Rules

- Do not put logic in `lib.rs` or `bindings/`.
- Put new code in the closest domain module; create a submodule only when a file
  gains a separate responsibility.
- Keep domain modules pure Rust where possible. NAPI types belong at the edge.
- Stateful orchestration that must persist across NAPI calls (LSP client pool,
  security registry) belongs in the TS tier (`src/lsp/*.ts`, `src/security/*.ts`),
  not Rust.
- Preserve root re-exports in `lib.rs` when moving modules to avoid churn in
  internal call sites.
- Avoid duplicate helpers across domains. Shared LSP command/path checks live in
  `src/lsp/commands.rs`.

## Cargo Deps

`package.json#version` is the release version source of truth for the engine.
`yarn version:sync` updates `Cargo.toml`, `Cargo.lock`, the root
`optionalDependencies`, and every `npm/<platform>/package.json` to match it.

- NAPI: `napi`, `napi-derive`; build: `napi-build`; dev: `napi`.
- Serialization/text: `serde`, `serde_json`, `serde_yaml_ng`, `regex`, `url`.
- Async/process/LSP: `tokio`, `lsp-types`, `which`.
- Search: `grep`, `ignore`.
- Minify/JS/CSS: `lightningcss`, `oxc_allocator`, `oxc_ast`, `oxc_codegen`,
  `oxc_minifier`, `oxc_parser`, `oxc_span`, `oxc_semantic`.
- Structural search: `tree-sitter`, `tree-sitter-language`.
- Grammars: `tree-sitter-typescript`, `tree-sitter-javascript`,
  `tree-sitter-python`, `tree-sitter-go`, `tree-sitter-rust`,
  `tree-sitter-java`, `tree-sitter-c`, `tree-sitter-cpp`,
  `tree-sitter-c-sharp`, `tree-sitter-bash`, `tree-sitter-json`,
  `tree-sitter-yaml`, `tree-sitter-toml-ng`, `tree-sitter-html`,
  `tree-sitter-css`, `tree-sitter-scss`, `tree-sitter-less`,
  `tree-sitter-scala`.

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

## Verification

Run from `packages/octocode-engine/`:

```bash
yarn version:sync
yarn build:all
yarn prepublish:verify
yarn verify:rust
yarn verify
```
