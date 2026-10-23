# Code graph architecture

The native `crates/engine` crate owns the reusable code graph model and algorithms. Runtime packages own path policy, cancellation, language-server lifecycle, pagination, redaction, and public tool response shaping.

## Models

One graph is queried and stored; the build has one intermediate:

- `BuiltGraph` (`runtime/src/tools/ast_graph/types.rs`): the linker's output for one scan, per-file facts plus the file graph (target file and edge kinds with their first line). Ingest projects it and drops it; nothing stores or queries it.
- `GraphTables` (`ast_graph/store/format.rs`): file, symbol and package nodes joined by `contains`, `imports`, `uses`, `calls` and `inherits` edges, with diagnostics, file digests, components and entries. It is the snapshot `graph.bin` holds and the one model every `graph query` operation, `drift` included, reads.

The graph-facts scan (`engine/src/fact_scan/`) crosses the Rust package boundary as `GraphFactsMappedScan<T>`: `scan_walked_graph_facts` takes a `walk_graph_fact_files` walk, hands each parsed file to the caller's map as a borrowed `GraphFactsView`, and keeps only what the map returns. `FactsSource` picks full graph facts (`Graph`) or the caller's cached declarations (`Declarations`).

## Ingestion flow

1. AST extraction produces typed declarations, imports, exports, calls, modules, ranges, and source diagnostics (a code, a message and an optional line; facts schema v2) for every language in the [grammar registry](SUPPORTED_LANGUAGES_AND_FEATURES.md#language-matrix).
2. `build_graph` (`ast_graph/graph.rs`) scans a `GraphScan` (root, pruned directories, file bound, language globs, Cargo linking) and links each file's imports, re-exports, calls and paths into a `BuiltGraph`.
3. `tables::project` turns it into `GraphTables`; `format::encode` writes them.

The graph is syntax evidence: `astSearch` and `graph` start no language server. Callers confirm identity with explicit `lspSearch` calls, which keeps syntax candidates distinct from semantic claims.

## Cycle engine

`ast_graph/algorithms.rs` holds the one cycle engine (`Cycles`): the dependency view of a file graph (data and module-tree edges removed), its strongly connected groups, and the groups its eager-load (runtime) edges close. `graph query cycles`, the `cycle` detector, `drift` and the health summary read it. The engine crate supplies the primitives (`engine/src/graph/algorithms.rs`): strongly connected components (iterative Tarjan) and the condensation.

## Persisted graphs (CLI)

`npx octocode graph ingest <path>` runs the `build_graph` linker. It projects the result into file, symbol, and package nodes with
`contains`, `imports`, `uses`, `calls`, and `inherits` edges, and publishes an immutable snapshot:

```text
<workspace>/.octocode/graph/
  latest                         # id of the newest snapshot
  <YYYYMMDDTHHMMSSZ>-<scope>-<root tag>/
    manifest.json                # root, scope, counts, link tallies, diagnostics,
                                 # linking-config digests (inputs), sha256
    graph.bin
```

The root tag is 6 hex digits of the scanned root, so two roots whose scopes
slug alike (`a-b`, `a/b`) never share an id, and pruning keeps snapshots per
root. Before any read, hash or `git` call, a snapshot's manifest root must be
canonical and inside the allowed roots.

Freshness has one owner (`ast_graph/freshness.rs`). It covers every source and the linking configs that change
resolution (tsconfig/jsconfig, `package.json`, `Cargo.toml`, `go.mod`, and the
like). Both ingest reuse and `graph query stale` use it: `stale` rescans the
scoped file list, so it reports `changed`, `missing`, `added` and `excluded`
sources and changed configs. A snapshot truncated at its file limit is never
reported fresh.

`graph.bin` (format v6) is a sectioned, varint-coded file:

- **Header:** magic `OCGRAPH\0`, format version, section count, and the
  SHA-256 of the body.
- **Section table:** `{tag, offset, len}` entries.
- **`STRS`:** sorted unique strings, front-coded. Symbols store only their key
  suffix (`Struct.member`), and the reader rebuilds `path#Struct.member`, so
  every path is stored once.
- **`NODE` and `EDGE`:** varint columns. Edges are delta-coded by source, and
  kind, confidence and a test bit (every site of the edge is test code: a
  test file, or a Rust `#[cfg(test)]` item) share one byte.
- **`KEYX` / `NAMX`:** lookup permutations.
- **`DIAG`, `FDIG`, `FCMP`, `ENTR`:** diagnostics (file, line, code,
  message, and the file node a diagnostic is about, when that is another
  file), per-file digests, components, and entrypoints.
- **`FSTM`** (optional): per-file size, mtime, ctime and inode, recorded only
  for files untouched for 2 seconds (`stamp::SourceStamp::SETTLE`) before the scan. An unchanged-tree re-ingest
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
is what lets `--baseline` diff two snapshots. `--entry a,b` names the
entrypoints in place of the inferred ones (rule `explicit`) for every detector
that reads them.

`graph query drift --baseline <snapshot>` (`store/drift.rs`) diffs the
selected snapshot against another by root-relative key, so a checkout of the
base branch and the head compare: files added, removed and changed (content
digest), cycle groups added and resolved, file import pairs whose load turned
from module-load to deferred or back (`staticToDynamic`, `dynamicToStatic`),
and relations `(from, edge kind, to)` added and removed. A transition is not
also a removed and an added relation, and a moved line changes no relation.

`dependents <file>` with the default edges also lists the files that use the
file's exports through a re-exporting module (`edge: uses, via: reexport`).
A bare JS/TS import that names a scanned file by path (`vs/base/common/x` for
`src/vs/base/common/x.ts`) but that no tsconfig `paths`/`baseUrl` or workspace
package links is no edge. Ingest records it as an `unlinked-path-import`
diagnostic naming that file, and `dependents <file>` lists those importers
under `summary.unlinkedPathImports` (`file`, `line`, `message`).
Every answer's `graph` block carries `partialReasons` (`maxFiles`, `edgeCap`,
`filesSkipped`) when the snapshot's scan was cut, and `reingest`, the widened
ingest, when `maxFiles` cut it.

Large inputs are bounded:

- **Over 1 MB:** a file is never parsed. It stays a file node with the
  `unparsed` role, and relative imports of it link through
  `unparsed-target` edges. The false unresolved diagnostics those imports would
  otherwise raise are dropped.
- **Bundles under the bound:** a file classified as bundled (minified,
  `sourceMappingURL`, webpack/parcel runtime, build output) keeps its file node
  and imports. Its symbols are dropped and its call sites are not linked.

### Known limits

- No LSP-enriched call identity: method calls whose receiver type is not
  locally evident stay unlinked rather than guessed. The answers that follow
  calls report `coverage.callInternalRecall` so agents can see the gap.
- No incremental re-ingest: a changed tree is ingested again (an unchanged
  tree is reused whole).
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

Timing is a criterion benchmark, not a CI threshold. It builds and queries the same frozen graph (`OCTOCODE_GRAPH_BENCH_FILES`, default 20,000 files, and `OCTOCODE_GRAPH_BENCH_FANOUT`, default 4):

```bash
cargo bench --manifest-path packages/octocode-native/Cargo.toml -p octocode-engine --bench graph
```

Measure any proposed graph library or incremental parsing against the same correctness test and benchmark before adopting it.
