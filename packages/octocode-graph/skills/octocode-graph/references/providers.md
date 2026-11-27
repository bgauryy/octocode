Load when attaching coverage, traces, bundle records, profiles, data-flow or control-flow artifacts to a saved graph (`enrich`), or mapping a minified frame (`locate`).

# Attach measurements to a saved graph

Run `octocode-graph enrich <data.json> <manifest.json> <new-data.json>` from an installed graph package. The new export carries the original graph and a separate evidence layer. Query it with `octocode-graph query <new-data.json> "<query>" --json`.

| Artifact | What can be queried |
| --- | --- |
| `scip-json` | A decoded SCIP Index in protobuf JSON form. Document occurrences yield file-to-symbol `DEFINES`, `REFERENCES`, `READS` and `WRITES`. Document symbol relationships yield `IMPLEMENTS`, `REFERENCES`, `DEFINITION` and `TYPE_DEFINITION`. Document-local identifiers include the file name. Binary SCIP needs decoding first. |
| `llvm-cov` | Use `llvm-cov export` for one test per input. Set `testId`. The query `trace test:addition COVERS` returns files with positive, counted, non-gap segments. Each witness retains the segment coordinates and counts. |
| `otlp-json` | Supply OTLP/JSON trace export requests with `resourceSpans` and a `workload` name. `trace workload:checkout OBSERVED PARENT_OF EXECUTES_FILE` follows an observed parent to a child with a source-file attribute. Elapsed nanoseconds remain decimal strings. Spans without source attributes can still establish parentage. |
| `esbuild-metafile` | The `metafile` of an esbuild build. `bundle` lists its outputs; [what ships](provider-bundle.md#what-a-bundle-ships). |
| `rollup-bundle` | A rollup or rolldown output bundle, dumped by the plugin below. Module sizes are rendered lengths in characters. |
| `rolldown-analyze` | rolldown's `bundleAnalyzerPlugin` `analyze-data.json`. Its module sizes are the code before rendering, not bytes in the output: `bundle` says so and states no bundler overhead. |
| `webpack-stats` | webpack's stats JSON (`webpack --json > stats.json`, `stats.toJson()`): chunks with their emitted files, modules with their sizes (after loaders, before minification: measure `module`), concatenated modules counted by their parts, `import()` children as dynamic loads. |
| `codeql-sarif` | A SARIF 2.1.0 log of CodeQL path queries (`codeql database analyze … --format=sarif-latest`). Each thread flow of a result's `codeFlows` becomes `FLOWS_TO` steps; [data flow](provider-data-flow.md#data-flow). |
| `joern-flows` | Joern `reachableByFlows` paths as JSON, one element per step with the flow table's columns (`nodeType`, `tracked`, `lineNumber`, `method`, `file`); [data flow](provider-data-flow.md#data-flow). |
| `cpuprofile` | A V8 `.cpuprofile` (`node --cpu-prof`, Chrome DevTools). `hot` ranks files and functions by time; [where the cost lands](provider-profiles.md#where-profile-cost-lands). |
| `heapprofile` | A V8 sampling heap profile (`node --heap-prof`, `.heapprofile`). `hot --heap` ranks by sampled bytes. |

The result's `steps` carry the artifact name, relationship and available source coordinates. Test coverage describes the measured test. Runtime observations describe the supplied workload. Missing files, uninstrumented spans and unindexed documents limit conclusions about absence.

Write the manifest: [manifest](provider-manifest.md). LLVM control flow and dominance (`llvm-graphviz`): [LLVM](provider-llvm.md). A minified stack or profile frame: [locate](provider-profiles.md#map-minified-positions).
