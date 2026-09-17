# Code graph architecture

`octocode-engine` owns the reusable code graph model and algorithms. Runtime packages own path policy, cancellation, language-server lifecycle, pagination, redaction, and public tool response shaping.

## Evidence model

`CodeGraphSnapshot` is an immutable, deterministic evidence snapshot. It doesn't claim that syntax and language-server observations are one infallible semantic truth.

- `CodeNode` uses domain IDs such as `file:…`, `symbol:…`, and `occurrence:…`. Raw tree-sitter nodes, opaque LSP `data`, and graph-library indexes aren't durable IDs.
- `CodeEdge` links domain IDs and retains one or more `EvidenceId` values.
- `EvidenceSource::Ast` and `EvidenceSource::Lsp` remain distinct. LSP evidence includes the method, server receipt, advertised capabilities, and synchronized document version.
- `SnapshotMetadata` includes normalized source paths, content digests, the graph-fact schema, a generation, and a canonical snapshot digest.
- `GraphCompleteness` and `CodeGraphDiagnostic` represent skipped files, unsupported syntax, unavailable semantic enrichment, and other incomplete states.

The implementation is in `src/graph/model.rs`. Native graph-fact extraction crosses the Rust package boundary as `GraphFactsTypedScanResult`; the JSON form remains only as a compatibility adapter for N-API and TypeScript consumers.

## Ingestion flow

1. AST extraction produces typed declarations, imports, exports, calls, modules, ranges, and source diagnostics.
2. `CodeGraphBuilder::ingest_facts` adds syntax nodes and evidence.
3. The native linker adds resolved file relations without replacing extraction evidence.
4. A semantic orchestrator can add LSP relations only when its generation matches the source snapshot.
5. `CodeGraphBuilder::finish` sorts diagnostics and computes the immutable snapshot digest.

`NativeLspClient::graph_server_receipt` and `NativeLspClient::document_version` provide provenance for semantic ingestion. The client rejects unsupported position encodings, collects bounded LSP partial results, and rolls back document versions when synchronization fails.

## File topology algorithms

Reusable deterministic algorithms are in `src/graph/algorithms.rs`:

- forward and reverse traversal
- shortest path
- strongly connected components and cycle witnesses
- condensation and topological layers
- transitive-edge detection

`octocode-native/src/tools/ast_graph/algorithms.rs` only re-exports these engine primitives. The public `astSearch` response remains a syntax-confidence file-topology contract; semantic evidence isn't relabeled as syntax or exposed as proven symbol identity.

## Evaluation

Correctness gates cover canonical digests, stale semantic evidence, typed-fact fidelity, reverse-graph invariants, SCC partitioning, and native response parity. Run:

```bash
cargo test --manifest-path packages/octocode-engine/Cargo.toml --no-default-features graph::
cargo test --manifest-path packages/octocode-engine/Cargo.toml --no-default-features --test graph_benchmark frozen_graph_correctness_receipt_is_stable -- --exact
cargo test --manifest-path packages/octocode-native/Cargo.toml tools::ast_graph
```

A manual sensor reports file and edge counts, file-topology build time, immutable-snapshot build time, and query time without imposing a flaky CI timing threshold:

```bash
cargo test --manifest-path packages/octocode-engine/Cargo.toml --no-default-features --test graph_benchmark measure_frozen_graph_baseline -- --ignored --exact --nocapture
```

The sensor writes `$TMPDIR/octocode-graph-benchmark.json`. Set `OCTOCODE_GRAPH_BENCH_REPORT` to choose another report path. Set `OCTOCODE_GRAPH_BENCH_FILES` and `OCTOCODE_GRAPH_BENCH_FANOUT` to scale the frozen graph. Compare any proposed graph library against the same correctness fixture and sensor before adoption. Keep `BTreeMap` and full snapshot rebuilds unless a held-out benchmark demonstrates a material correctness, memory, or latency gain from another representation or incremental parsing.
