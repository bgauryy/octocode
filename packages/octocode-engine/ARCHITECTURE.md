# Octocode engine architecture

`@octocodeai/octocode-engine` is the reusable Rust algorithms crate. It is linked directly by `octocode-native` and also published as a Node N-API package for primitive-level consumers.

```text
octocode-native ── Rust rlib ──▶ octocode-engine domains
Node consumers ── loader ──▶ platform N-API addon ── thin bindings ──▶ same domains
```

The engine does not own public Octocode tool contracts, provider behavior, credentials, policy composition, pagination, or response rendering. Those belong to `octocode-native`.

## Boundaries

- `src/lib.rs` declares modules and explicit Rust/N-API exports.
- `src/bindings/` converts N-API values, invokes a domain function, and maps errors. It contains no independent implementation.
- `src/portable.rs` exposes typed Rust entry points used by `octocode-native`.
- `src/types.rs` holds shared binding-safe data structures.
- Filesystem mutation and edit-preview diff primitives for the Pi extension belong to `octocode-extension-rust`; this crate retains research diff filtering and structural rewrite primitives.
- JavaScript at the package root only selects and loads the platform addon. There is no TypeScript security, LSP, or execution tier.

## Domains

- `src/search/` owns ripgrep-compatible lexical and filesystem primitives with bounded traversal and explicit completion state.
- `src/structural/` owns embedded AST search, syntax trees, rules, transforms, rewriters, file discovery, and structural rewrite execution.
- `src/signatures/` owns language grammars, signatures, symbols, references, and graph-fact extraction.
- `src/lsp/` owns command discovery, workspace resolution, JSON-RPC transport, semantic position resolution, validation, diagnostics, and the complete pooled language-server lifecycle.
- `src/security/` owns the canonical secret-pattern set, detection, and content sanitization primitives.
- `src/graph/` owns graph scans, immutable snapshots, diffs, policy primitives, and deterministic graph algorithms.
- `src/index/` owns content-addressed generations, freshness, integrity, quotas, and indexed search.
- `src/minify/` and `src/text/` own content reduction, UTF-8/UTF-16 offsets, diff parsing, file classification, and YAML emission.

## Graph evidence

`astSearch` topology operations consume engine facts but are orchestrated and rendered by `octocode-native`:

- JavaScript and TypeScript facts come from Oxc; other supported languages use Tree-sitter.
- Per-file facts normalize declarations, imports, exports, calls, classes, functions, ranges, and conservative same-file reference counts.
- Graph completeness records unsupported syntax, skipped files, unavailable semantic enrichment, and scan limits rather than treating incomplete evidence as absence.
- LSP supplies semantic identity, references, definitions, implementations, callers, callees, and call hierarchy. Topology edges remain candidates until semantic evidence confirms a deletion claim.

See [Code graph architecture](docs/CODE_GRAPH.md).

## Runtime invariants

- Every filesystem walk, parser, search, index, and LSP operation is bounded and cancellation-aware.
- Native LSP pool keys include all effective configuration. Starts and health checks deduplicate; active requests prevent idle shutdown; cleanup terminates owned processes.
- Location links preserve provider selection ranges separately from source context.
- Structural prefilters may exclude only impossible candidates; exhaustion is explicit and never presented as complete absence.
- Structural rewrite is embedded. External AST or rewrite executables are not discovered or launched.
- Security pattern order is Rust-owned and deterministic.
- Domain errors remain typed until the consuming runtime maps them into a public contract.

## Distribution

The root package ships `index.js`, `index.cjs`, `index.d.ts`, documentation, and loader metadata. Six optional platform packages each ship one `octocode-engine.<platform>.node` addon. The root package contains no platform binary.

`loader/` is the canonical source for the ESM, CommonJS, and declaration entry points. Napi-rs temporarily overwrites root entries during a build; `scripts/postbuild.cjs` snapshots the generated ABI and restores the canonical loaders. `scripts/check-napi-abi.cjs` rejects drift between Rust exports and hand-authored declarations.

`package.json#version` is the release source of truth. `yarn version:sync` updates Cargo and platform-package versions. Platform packages publish before the root package.

## Rules

- Put reusable algorithms in the closest Rust domain, never in bindings or loaders.
- Keep public tool decisions in `octocode-native`; engine functions expose evidence and explicit completion state.
- Declare exports explicitly and avoid duplicate adapters or wildcard relay modules.
- Do not add a TypeScript fallback for a Rust primitive.
- Keep external processes limited to configured language servers; public Git cloning is owned by `octocode-native`.

## Verification

```bash
yarn version:sync
yarn build:dev
yarn typecheck
yarn test
yarn verify:rust
yarn loader:check
yarn verify:napi-abi
```
