# Attach measurements to a saved graph

Run `octocode-graph enrich <data.json> <manifest.json> <new-data.json>` from an installed graph package. The new export carries the original graph and a separate evidence layer. Query it with `octocode-graph query <new-data.json> "<query>" --json`.

Choose artifacts collected from the same source revision and build configuration, then write the manifest with one `--provider` per artifact; each option after a `--provider` belongs to it:

```sh
octocode-graph enrich --manifest-from data.json manifest.json --revision build-42 \
  --provider llvm-cov coverage.json --test-id addition --source-root /build/project
```

It records the SHA-256 of the export (`baseSha256`) and of each file (`sha256`) as they are on disk, names each file relative to the manifest, refuses to replace an existing manifest, and prints the `enrich` command to run next. Provider options: `--test-id`, `--workload`, `--source-root`, `--working-directory`, `--module-file`. The importer checks both digests and the revision labels. The producer attests which revision the files describe. The manifest it writes:

```json
{
  "revision": "build-42",
  "baseSha256": "<digest of data.json>",
  "providers": [
    {
      "format": "llvm-cov",
      "file": "coverage.json",
      "sha256": "<digest of coverage.json>",
      "revision": "build-42",
      "sourceRoot": "/build/project",
      "testId": "addition"
    }
  ]
}
```

Artifact paths resolve relative to the manifest. Set `sourceRoot` to the absolute repository root on the machine that produced the measurement if it differs from the export's `section`. Keep separate output names for separate manifests. The importer refuses to replace an existing output.

| Artifact | What can be queried |
| --- | --- |
| `scip-json` | A decoded SCIP Index in protobuf JSON form. Document occurrences yield file-to-symbol `DEFINES`, `REFERENCES`, `READS` and `WRITES`. Document symbol relationships yield `IMPLEMENTS`, `REFERENCES`, `DEFINITION` and `TYPE_DEFINITION`. Document-local identifiers include the file name. Binary SCIP needs decoding first. |
| `llvm-cov` | Use `llvm-cov export` for one test per input. Set `testId`. The query `trace test:addition COVERS` returns files with positive, counted, non-gap segments. Each witness retains the segment coordinates and counts. |
| `otlp-json` | Supply OTLP/JSON trace export requests with `resourceSpans` and a `workload` name. `trace workload:checkout OBSERVED PARENT_OF EXECUTES_FILE` follows an observed parent to a child with a source-file attribute. Elapsed nanoseconds remain decimal strings. Spans without source attributes can still establish parentage. |
| `esbuild-metafile` | The `metafile` of an esbuild build. `bundle` lists its outputs; [what ships](#what-a-bundle-ships). |
| `rollup-bundle` | A rollup or rolldown output bundle, dumped by the plugin below. Module sizes are rendered lengths in characters. |
| `rolldown-analyze` | rolldown's `bundleAnalyzerPlugin` `analyze-data.json`. Its module sizes are the code before rendering, not bytes in the output: `bundle` says so and states no bundler overhead. |
| `webpack-stats` | webpack's stats JSON (`webpack --json > stats.json`, `stats.toJson()`): chunks with their emitted files, modules with their sizes (after loaders, before minification: measure `module`), concatenated modules counted by their parts, `import()` children as dynamic loads. |
| `codeql-sarif` | A SARIF 2.1.0 log of CodeQL path queries (`codeql database analyze … --format=sarif-latest`). Each thread flow of a result's `codeFlows` becomes `FLOWS_TO` steps; [data flow](#data-flow). |
| `joern-flows` | Joern `reachableByFlows` paths as JSON, one element per step with the flow table's columns (`nodeType`, `tracked`, `lineNumber`, `method`, `file`); [data flow](#data-flow). |
| `cpuprofile` | A V8 `.cpuprofile` (`node --cpu-prof`, Chrome DevTools). `hot` ranks files and functions by time; [where the cost lands](#where-profile-cost-lands). |
| `heapprofile` | A V8 sampling heap profile (`node --heap-prof`, `.heapprofile`). `hot --heap` ranks by sampled bytes. |

The result's `steps` carry the artifact name, relationship and available source coordinates. Test coverage describes the measured test. Runtime observations describe the supplied workload. Missing files, uninstrumented spans and unindexed documents limit conclusions about absence.

## LLVM control flow and dominance

Compile a translation unit to LLVM IR with block names. Run the two LLVM passes on the **same** IR, then convert their DOT outputs to Graphviz JSON. For a function named `choose`:

```sh
clang -S -emit-llvm -fno-discard-value-names -O0 choose.c -o choose.ll
opt -passes='dot-cfg-only,dot-dom-only' -disable-output choose.ll
dot -Tjson0 .choose.dot -o cfg.json
dot -Tjson0 domonly.choose.dot -o dom.json
```

Pass each JSON file as `--provider llvm-graphviz <file.json> --module-file choose.ll`: the manifest records the module's `moduleSha256` beside the file's digest. The importer qualifies each block by module digest, function name and block name. The control-flow and dominator printers may assign different Graphviz node IDs.

From `llvm:<moduleSha256>`, trace `DEFINES CONTAINS` to list block IDs. From a block, trace `BRANCHES_TO+` for possible successor blocks or `IMMEDIATELY_DOMINATES+` for blocks it strictly dominates. Edges cite the supplying graph artifact. These are compiler basic blocks for the supplied IR and optimization settings. Instruction-level data flow requires separate facts.

## What a bundle ships

The source graph shows what the code imports; a bundle shows what shipped after tree-shaking, code splitting and packages the graph cannot see into. Attach the build's own record:

- esbuild: `esbuild src/worker.ts --bundle --metafile=dist/meta.json --outdir=dist` (or `metafile: true` in the API and write `result.metafile`).
- rolldown: `bundleAnalyzerPlugin()` from `rolldown/experimental` writes `dist/analyze-data.json`; format `rolldown-analyze`.
- rollup, or rolldown with bytes in output: add this plugin and attach `bundle.json` as `rollup-bundle`.

```js
import { writeFileSync } from 'node:fs';
export const dumpBundle = { name: 'dump-bundle', generateBundle(_, bundle) { writeFileSync('bundle.json', JSON.stringify(Object.values(bundle).map(o => o.type === 'asset'
  ? { type: 'asset', fileName: o.fileName, bytes: Buffer.byteLength(o.source) }
  : { type: 'chunk', fileName: o.fileName, isEntry: o.isEntry, isDynamicEntry: o.isDynamicEntry, facadeModuleId: o.facadeModuleId, imports: o.imports, dynamicImports: o.dynamicImports,
      bytes: Buffer.byteLength(o.code), modules: Object.fromEntries(Object.entries(o.modules).map(([id, m]) => [id, { renderedLength: m.renderedLength }])) }))); } };
```

Relative paths in a metafile start from the bundler's working directory: set the provider's `workingDirectory` (absolute, on the machine that built) when it is not the graph root. Inputs become root-relative paths, `node_modules/…` included; virtual modules keep their names (`\0` is written `\\0`). rollup and rolldown name outputs by file name, relative to their output directory.

| Query | Answer |
| --- | --- |
| `bundle` | Every output with its bytes, kind (`entry`, `dynamic-entry`, `chunk`, `asset`) and entry file, heaviest first. |
| `bundle <output>` | The output's bytes by source file, heaviest first, then by package (`pkg:<name>` from `node_modules`, or the graph's workspace package; `(virtual)`, `(outside root)`). `stats.overhead` is the output's bytes no input accounts for (bundler code, padding); `stats.loads` the chunks it loads; `stats.externals` what it leaves to the runtime. An output is its path or a unique suffix. |
| `bundle <output> --diff` | The output and every chunk it loads statically, against `reach <entry> --runtime`. `tree-shaken`: reached, read by the bundler, no byte shipped. `not shipped`: reached, named by no input of this build (never read, or a format that lists only rendered modules). `unexpected`: shipped but not reached: an edge the graph missed, a file outside the graph, a package only another package pulls in; `importedBy` is the bundler's importer. `package`: a file of a package the reached files import, which the graph does not see into. Reached files in chunks loaded by `import()` are no difference (`stats.dynamicFiles`). Dropped rows carry their source bytes, shipped rows their bytes in output. |

`trace build:<artifact> OUTPUTS BUNDLES` walks the same evidence: `OUTPUTS`, `BUNDLES`, `LOADS`, `INPUTS` (each input's own size), `BUNDLE_IMPORTS` (the bundler's module graph) and `EXTERNAL`.

A build also finds these files by itself, without `enrich`: a metafile, stats or bundle dump (`*meta*.json`, `*stats*.json`, `analyze-data.json`, `*bundle*.json`, recognized by shape) in `.`, `dist`, `build`, `out`, `output`, `.output`, `stats` or `reports` (and one level below) of the root or a chunk. Each external package then gets its bytes in the bundle; a package no bundle holds gets its installed closure: its entry as Node resolves it from the importing file, plus every file its static imports, `import()` and `require()` load across installed packages. `data.json` `externals[]` carries `bytes`, `bytesMethod` (`bundle` or `installed-closure`) and `bytesEvidence` (the file it came from, files counted, version, unresolved specifiers); a builtin, an uninstalled package or another ecosystem's package keeps `bytes: null` with `bytesEvidence.reason`. A closure counts what loading the package can read, before tree-shaking: on d3, json-schema-to-typescript and 3d-force-graph it was 1.07×, 0.89× and 1.55× their bytes in an unminified esbuild bundle.

## Data flow

Data flow arrives as evidence from the tools that compute it. Run a CodeQL path query (`@kind path-problem`, such as `js/sql-injection`) and attach its SARIF as `codeql-sarif`; the provider's `revision` must be the commit the log names in `versionControlProvenance` (an abbreviation of it agrees), or the import is refused. Relative URIs are relative to the analyzed checkout (`%SRCROOT%`): set `sourceRoot` when that is not the graph's root on this machine. Joern flows attach as `joern-flows`: a list of flows, each `{ "name": "<query>", "elements": [{ "nodeType", "tracked", "lineNumber", "columnNumber"?, "method", "file" }] }`.

Each step is a node `<file>#L<line>:<column>`, so two flows meet only at one program point. Its file starts the flow, and each step `FLOWS_TO` the next, carrying the rule id, the result's message and both steps' locations and messages (`details.from`, `details.to`, `role`: `source`, `step` or `sink`).

| Query | Answer |
| --- | --- |
| `trace src/api.js FLOWS_TO+` | Every step reachable from the flows that start in the file, each cited at the line it arrives at; `steps` hold the witnesses with the rule id |
| `trace src/db.js#L8:16 FLOWS_TO` | The next step from one program point |

The flows are the analyzer's: a path it did not report is not absence of flow, and results without `codeFlows` (plain problems) are only counted (`withoutFlow` in the provider record).

## Where profile cost lands

Record with `node --cpu-prof --cpu-prof-dir=prof app.js` or `node --heap-prof …`, then attach the file as `cpuprofile` or `heapprofile`. Set `sourceRoot` when the profile was recorded under another root.

The importer maps each frame to a graph file and line: a graph file directly; a bundled file through its source map (its `sourceMappingURL`, else `<file>.map`, read beside the file under the graph root on this machine; the record lists each map with its digest). The innermost graph symbol whose lines hold the line names the function; a frame outside every symbol keys as `<file>:<line>`, a file's module scope as `<file>:(module)`. CPU cost is microseconds: each sample lasts until the next one (the last until `endTime`). Heap cost is sampled bytes. Total counts a file or function once per stack, so recursion is not counted twice.

| Query | Answer |
| --- | --- |
| `hot [files\|functions] [n\|all]` | Files (default) or functions by self cost, with total and their shares of busy time (sampled minus idle) or of sampled bytes. Rows cite `path:line`: a file's hottest function, a function's declaration. Row count: [ranked answers](query.md#ask-the-graph). |
| `hot … --from <file>` | Only files in `reach <file> --runtime`: the hot code of one flow. `stats.outsideReach` is the mapped self cost left out. |
| `hot … --heap` | The heap profiles; without it the CPU profiles answer when there are any. Several profiles of one kind add up. |

Cost the graph holds no file for is never dropped: `stats.unmapped` by reason (`(program)`, `(garbage collector)`, `(idle)`, `native`, `node internals`, `node_modules`, `outside root`, `not in graph`, `unmapped position`, `bundle module scope`, `other url`), `stats.unmappedPackages` by package and `stats.unmappedFiles` by file. `mapped` plus every unmapped reason is `sampled`. A bundled file without a readable source map counts as `not in graph` under its own path: add the map and enrich again. `trace profile:<artifact> SAMPLED` lists every mapped file and function with its cost.

## Map minified positions

`octocode-graph locate <bundle.js:line:col>... [--graph <result-dir|project-dir>] [--json]` maps positions in bundled or minified code, 1-based as stack traces print them, to the original `source:line:column` and name through the bundle's source map. With `--graph` it names the graph symbol whose lines hold the mapped line, with its declaration line. Without a map, or where the map maps nothing, it names the function around the generated position from the bundle text and lists the graph symbols of that name: one resolves by name only (`resolvedBy: "name"`), several stay candidates, none is said. A position past the text says so; an unreadable bundle exits 1. Index maps (`sections`) are refused.

```sh
octocode-graph locate dist/worker.min.js:2:81 --graph .octocode/octocode-graph/run
# dist/worker.min.js:2:81 → src/lib/parse.ts:12:17 checksum  in src/lib/parse.ts#checksum (declared at line 12)
```
