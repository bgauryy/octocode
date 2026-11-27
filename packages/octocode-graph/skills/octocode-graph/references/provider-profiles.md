Load when attaching a CPU or heap profile (`hot`) or mapping a minified position to its source (`locate`).

# Profiles and minified positions

## Where profile cost lands

Record with `node --cpu-prof --cpu-prof-dir=prof app.js` or `node --heap-prof …`, then attach the file as `cpuprofile` or `heapprofile`. Set `sourceRoot` when the profile was recorded under another root.

The importer maps each frame to a graph file and line: a graph file directly; a bundled file through its source map (its `sourceMappingURL`, else `<file>.map`, read beside the file under the graph root on this machine; the record lists each map with its digest). The innermost graph symbol whose lines hold the line names the function; a frame outside every symbol keys as `<file>:<line>`, a file's module scope as `<file>:(module)`. CPU cost is microseconds: each sample lasts until the next one (the last until `endTime`). Heap cost is sampled bytes. Total counts a file or function once per stack, so recursion is not counted twice.

| Query | Answer |
| --- | --- |
| `hot [files\|functions] [n\|all]` | Files (default) or functions by self cost, with total and their shares of busy time (sampled minus idle) or of sampled bytes. Rows cite `path:line`: a file's hottest function, a function's declaration. Row count: [ranked answers](query.md#walk-flags-and-row-counts). |
| `hot … --from <file>` | Only files in `reach <file> --runtime`: the hot code of one flow. `stats.outsideReach` is the mapped self cost left out. |
| `hot … --heap` | The heap profiles; without it the CPU profiles answer when there are any. Several profiles of one kind add up. |

Cost the graph holds no file for is never dropped: `stats.unmapped` by reason (`(program)`, `(garbage collector)`, `(idle)`, `native`, `node internals`, `node_modules`, `outside root`, `not in graph`, `unmapped position`, `bundle module scope`, `other url`), `stats.unmappedPackages` by package and `stats.unmappedFiles` by file. `mapped` plus every unmapped reason is `sampled`. A bundled file without a readable source map counts as `not in graph` under its own path: add the map and enrich again. `trace profile:<artifact> SAMPLED` lists every mapped file and function with its cost.

## Map minified positions

`octocode-graph locate <bundle.js:line:col>... [--graph <result-dir|project-dir>] [--json]` maps positions in bundled or minified code, 1-based as stack traces print them, to the original `source:line:column` and name through the bundle's source map. With `--graph` it names the graph symbol whose lines hold the mapped line, with its declaration line. Without a map, or where the map maps nothing, it names the function around the generated position from the bundle text and lists the graph symbols of that name: one resolves by name only (`resolvedBy: "name"`), several stay candidates, none is said. A position past the text says so; an unreadable bundle exits 1. Index maps (`sections`) are refused.

```sh
octocode-graph locate dist/worker.min.js:2:81 --graph .octocode/octocode-graph/run
# dist/worker.min.js:2:81 → src/lib/parse.ts:12:17 checksum  in src/lib/parse.ts#checksum (declared at line 12)
```
