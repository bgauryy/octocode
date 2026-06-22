# Octocode Engine Architecture

`octocode-engine` is a napi-rs native package. JavaScript calls thin NAPI
bindings; Rust modules own the actual logic and are tested with `cargo test`.

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
- `src/lsp/` owns native LSP support: client lifecycle, config, JSON-RPC,
  resolver, URI/path validation, workspace detection, and shared command checks.
- `src/signatures/` owns semantic outlines and JS/TS symbol extraction.
- `src/security/` owns secret detection and sanitization.

## Rules

- Do not put logic in `lib.rs` or `bindings/`.
- Put new code in the closest domain module; create a submodule only when a file
  gains a separate responsibility.
- Keep domain modules pure Rust where possible. NAPI types belong at the edge.
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
