# Code graph architecture

The native `crates/engine` crate owns the reusable code graph model and algorithms. Runtime packages own path policy, cancellation, language-server lifecycle, pagination, redaction, and public tool response shaping.

## Evidence model

`CodeGraphSnapshot` is an immutable, deterministic evidence snapshot. It doesn't claim that syntax and language-server observations are one infallible semantic truth.

- `CodeNode` uses domain IDs such as `file:…`, `symbol:…`, and `occurrence:…`. Raw tree-sitter nodes, opaque LSP `data`, and graph-library indexes aren't durable IDs.
- `CodeEdge` links domain IDs and retains one or more `EvidenceId` values.
- `EvidenceSource::Ast` and `EvidenceSource::Lsp` remain distinct. LSP evidence includes the method, server receipt, advertised capabilities, and synchronized document version.
- `SnapshotMetadata` includes normalized source paths, content digests, the graph-fact schema, a generation, and a canonical snapshot digest.
- `GraphCompleteness` and `CodeGraphDiagnostic` represent skipped files, unsupported syntax, unavailable semantic enrichment, and other incomplete states.

The implementation is in `src/graph/model.rs`. Native graph-fact extraction crosses the Rust package boundary as `GraphFactsTypedScanResult`; N-API consumers receive its serialized representation through the thin binding layer.

## Ingestion flow

1. AST extraction produces typed declarations, imports, exports, calls, modules, ranges, and source diagnostics for the canonical ten-language, 25-extension grammar registry.
2. `CodeGraphBuilder::ingest_facts` adds syntax nodes and evidence.
3. The native linker adds resolved file relations without replacing extraction evidence.
4. A semantic orchestrator can add LSP relations only when its generation matches the source snapshot.
5. `CodeGraphBuilder::finish` sorts diagnostics and computes the immutable snapshot digest.

`NativeLspClient::graph_server_receipt` and `NativeLspClient::document_version` provide provenance for semantic ingestion. The client rejects unsupported position encodings, collects bounded LSP partial results, and rolls back document versions when synchronization fails.

## Runtime integration status

Native topology construction retains the AST snapshot in `BuiltGraph::code_graph`. `astSearch` remains AST-only because the canonical tool contract has no semantic-enrichment request or budget field. The runtime must not start language servers silently and change latency or availability semantics.

A runtime that opts into enrichment through a future canonical contract must select bounded AST candidates, synchronize their source documents, verify capabilities, attach both the server receipt and the document version, and reject stale generations. It must then call either `mark_semantic_complete` or `mark_semantic_incomplete`. Until then, callers must combine `astSearch` candidates with explicit `lspSearch` proof. This keeps syntax candidates distinct from semantic claims.

## File topology algorithms

Reusable deterministic algorithms are in `src/graph/algorithms.rs`:

- forward and reverse traversal
- shortest path
- strongly connected components and cycle witnesses
- condensation and topological layers
- transitive-edge detection

`octocode-native/src/tools/ast_graph/algorithms.rs` only re-exports these engine primitives. The public `astTopology` response remains a syntax-confidence file-topology contract; semantic evidence isn't relabeled as syntax or exposed as proven symbol identity.

## Persisted graphs (CLI)

`octocode graph ingest <path>` runs the same `build_graph` linker as
`astTopology`. It projects the result into file, symbol, and package nodes with
`contains`, `imports`, and `calls` edges, and publishes an immutable snapshot:

```text
<workspace>/.octocode/graph/
  latest                         # id of the newest snapshot
  <YYYYMMDDTHHMMSSZ>-<scope>/
    manifest.json                # scope, counts, link tallies, diagnostics, sha256
    graph.bin
```

`graph.bin` (format v2) is a sectioned, varint-coded file:

- **Header:** magic `OCGRAPH\0`, format version, section count, and the
  SHA-256 of the body.
- **Section table:** `{tag, offset, len}` entries.
- **`STRS`:** sorted unique strings, front-coded. Symbols store only their key
  suffix (`Struct.member`), and the reader rebuilds `path#Struct.member`, so
  every path is stored once.
- **`NODE` and `EDGE`:** varint columns. Edges are delta-coded by source, and
  kind plus confidence share one byte.
- **`KEYX` / `NAMX`:** lookup permutations.
- **`DIAG`, `FDIG`, `FCMP`, `ENTR`:** diagnostics, per-file digests,
  components, and entrypoints.
- **Adjacency:** both CSR directions are rebuilt on load in O(V + E) instead
  of being stored.

It needs no dependencies and is deterministic. The only work at load is a linear adjacency rebuild. Decode
validates every cross-reference. An unknown major version asks the user to
re-ingest, and readers ignore unknown section tags.

Call edges are syntax candidates. Each one is linked in one of these ways, in
this order:

1. A same-file declaration (with receiver-aware member preference).
2. An import binding, following named and star re-exports for up to 8 hops.
3. A namespace or module qualifier (`ns.f`, `pkg.F`, `mod::f`).
4. A unique name across the whole graph (`low` confidence).

Every call edge keeps its `resolution` and `confidence`. `graph query` then
answers bounded, paged questions without rebuilding. The code is in
`crates/runtime/src/tools/ast_graph/store/`.

Ingest also classifies files by role, infers entrypoints, and reads manifests
into components and declared dependencies. That code is in `store/classify.rs`
and `store/workspace.rs`. Two more sections carry the results: `FCMP` maps each
file to its component and `ENTR` maps each entrypoint to the rule that found it.
Both are optional, so readers treat a missing section as empty. `uses` edges
link named imports to the exported declaration. `inherits` edges link a class,
interface, or impl to its base or trait.

`graph query issues` (`store/detect.rs`) runs the detectors on the loaded
snapshot. It needs no new dependencies:

- iterative Tarjan SCCs
- Eades–Lin–Smyth greedy feedback-arc cuts
- BFS witness cycles
- PageRank (damping 0.85, at most 50 iterations)
- articulation points with separated-side sizes
- Martin instability and abstractness
- Lakos CCD/NCCD over the SCC condensation, using bitset reachability

Finding ids hash the detector, the subject, and the identifying evidence, which
is what lets `--baseline` diff two snapshots.

Large inputs are bounded:

- **Over 1 MB:** a file is never parsed. It stays a file node with the
  `unparsed` role, and relative imports of it link through
  `unparsed-target` edges. The false unresolved diagnostics those imports would
  otherwise raise are dropped.
- **Bundles under the bound:** a file classified as bundled (minified,
  `sourceMappingURL`, webpack/parcel runtime, build output) keeps its file node
  and imports. Its symbols are dropped and its call sites are not linked.
- **Measured:** a 3.6 MB esbuild-minified bundle plus a 9 MB unminified file
  ingest in about 70 ms with 91 MB RSS.

### Validation

`scripts/graph-bench/bench.py` measures quality against the source text, not
against the graph's own output. On 20 repositories covering 12 grammars (47 to
50k files):

- Ingest is deterministic.
- 89–100% of sampled import edges and 98–100% of sampled high-confidence call
  edges verify against the source line they cite.
- Cycle witness hops verify at 80–100%.

`scripts/graph-bench/seeded.py` recalls 5/5 planted defects with zero
negative-control leaks. The controls are a lazy Python import cycle and a Rust
`mod tests` dev-dependency.

Not yet covered:

- LSP-enriched call identity. Method calls whose receiver type is not
  locally evident stay unlinked rather than guessed. The answers that follow
  calls report `coverage.callInternalRecall` so agents can see the gap.
- Incremental re-ingest of changed files (an unchanged tree is reused whole).

Symbols carry a test flag: symbols in test files, symbols inside Rust test
modules (`mod tests`, `mod proptests`, …), and pytest `test_*` functions and
`Test*` classes. `impact.testFunctions` lists the flagged symbols a change
reaches.

## Evaluation

Correctness gates cover canonical digests, stale semantic evidence, typed-fact fidelity, reverse-graph invariants, SCC partitioning, and native response parity. Run:

```bash
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --no-default-features graph::
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --no-default-features --test graph_benchmark frozen_graph_correctness_receipt_is_stable -- --exact
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-native tools::ast_graph
```

A manual sensor reports file and edge counts, file-topology build time, immutable-snapshot build time, and query time without imposing a flaky CI timing threshold:

```bash
cargo test --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --release --no-default-features --test graph_benchmark measure_frozen_graph_baseline -- --ignored --exact --nocapture
```

The release profile keeps debug instrumentation from distorting the receipt. The sensor writes `$TMPDIR/octocode-graph-benchmark.json`. Set `OCTOCODE_GRAPH_BENCH_REPORT` to choose another report path. Set `OCTOCODE_GRAPH_BENCH_FILES` and `OCTOCODE_GRAPH_BENCH_FANOUT` to scale the frozen graph. Compare any proposed graph library against the same correctness fixture and sensor before adoption. Keep `BTreeMap` and full snapshot rebuilds unless a held-out benchmark demonstrates a material correctness, memory, or latency gain from another representation or incremental parsing.
