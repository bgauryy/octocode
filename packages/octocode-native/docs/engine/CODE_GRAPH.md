# Code graph architecture

The native `crates/engine` crate owns the reusable code graph model and algorithms. Runtime packages own path policy, cancellation, language-server lifecycle, pagination, redaction, and public tool response shaping.

## Evidence model

`CodeGraphSnapshot` is an immutable, deterministic syntax-evidence snapshot. `astTopology drift` builds one for the baseline and one for the head and diffs them with `diff_graphs`; no other operation builds it.

- `CodeNode` uses domain IDs such as `file:…`, `symbol:…`, and `occurrence:…`. Raw tree-sitter nodes and graph-library indexes aren't durable IDs.
- `CodeEdge` links domain IDs and retains one or more `EvidenceId` values. Every edge is AST evidence (`EvidenceSource::Ast`); relation identity is `(from, kind, to)`, so a line shift changes evidence, not relations.
- `SnapshotMetadata` holds the scan root, the graph-fact schema, and each normalized source path with its content digest.
- `GraphCompleteness` records whether the scan was complete and why not.

The implementation is in `src/graph/model.rs` and `src/graph/diff.rs`. Native graph-fact extraction crosses the Rust package boundary as `GraphFactsTypedScanResult`.

## Ingestion flow

1. AST extraction produces typed declarations, imports, exports, calls, modules, ranges, and source diagnostics for the canonical ten-language, 25-extension grammar registry.
2. `CodeGraphBuilder::ingest_facts` adds syntax nodes and evidence.
3. The native linker adds resolved file relations without replacing extraction evidence.
4. `CodeGraphBuilder::finish` returns the snapshot.

## Runtime integration status

Native topology construction retains the AST snapshot in `BuiltGraph::code_graph` for `drift` only. `astSearch` and `astTopology` remain AST-only: the canonical tool contract has no semantic-enrichment request or budget field, and the runtime must not start language servers silently and change latency or availability semantics. Callers combine `astSearch` candidates with explicit `lspSearch` proof, which keeps syntax candidates distinct from semantic claims.

## File topology algorithms

Reusable deterministic algorithms are in `src/graph/algorithms.rs`:

- forward and reverse traversal
- shortest path
- strongly connected components and cycle witnesses
- condensation and topological layers
- transitive-edge detection

`octocode-native/src/tools/ast_graph/algorithms.rs` only re-exports these engine primitives. The public `astTopology` response remains a syntax-confidence file-topology contract; semantic evidence isn't relabeled as syntax or exposed as proven symbol identity.

## Persisted graphs (CLI)

`npx octocode graph ingest <path>` runs the same `build_graph` linker as
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
- **`FSTM`** (optional): per-file size, mtime, ctime and inode, recorded only
  for files untouched for 2 seconds before the scan. An unchanged-tree re-ingest
  compares these stamps before hashing a source; a file without a stamp, or with
  a different one, is hashed.
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
- C/C++ unreachable-file precision stays low: headers reach the build through
  `-I` roots and Kbuild/CMake rules, which ingest does not parse.
- Rust call recall is capped by method chains and function return types;
  receiver facts do not propagate return types.

Symbols carry a test flag: symbols in test files, symbols inside Rust test
modules (`mod tests`, `mod proptests`, …), and pytest `test_*` functions and
`Test*` classes. `impact.testFunctions` lists the flagged symbols a change
reaches.

CLI graph queries page affected rows with `--limit` and `--offset`. Large
`impact` summary arrays and `issues` baseline arrays show 100 entries in the
initial response and provide one executable `nextLists` command per array.
For example, `--list summary.testsToRun` returns the complete test list as
ordinary paged `results`; follow its `next` command until it ends. A cycle
larger than 50 nodes includes `nextNodes`, which pages every member through
`--list cycleNodes`. Generated continuations retain an explicit `--workspace`
and pin the graph snapshot ID.

Snapshot reads are bounded at 512 MiB. Decode permits at most 64 sections,
5 million items per table, 2 million edges, and 512 MiB of expanded string
content. A snapshot beyond these limits fails with a narrow-scope or
re-ingest error instead of exhausting memory.

## Evaluation

Correctness gates cover typed-fact fidelity, structural diff identity, reverse-graph invariants, SCC partitioning, and native response parity. Run:

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
