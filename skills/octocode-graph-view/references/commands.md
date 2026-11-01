Load when building, serving or replaying a graph, reading events, or changing a script.

# Commands

Run from the target workspace (octocode resolves paths and allowed roots from it). `--help` on each script owns the accepted flags. Inside the octocode checkout the scripts run the native binary the local launcher would start, found by the launcher's own resolver (`packages/octocode-native/bin/resolve-binary.cjs`), which saves the launcher's Node startup on every call; `OCTOCODE_CLI` overrides.

| Goal | Command |
|---|---|
| What runs here | `node <skill>/scripts/preflight.mjs [--json]` |
| Live viewer | `node <skill>/scripts/serve.mjs <section> [--out <dir>] [--whole] [--lsp on\|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--port N] [--no-open] [--exit-when-done]` |
| Headless result | `node <skill>/scripts/build.mjs <section> [same flags] [--progress]` |
| Reopen a finished run | `node <skill>/scripts/serve.mjs --replay <result-dir>` |
| Briefs again (after `model.json`) | `node <skill>/scripts/render.mjs --data <dir>/data.json` |

## Pipeline

1. **Files**: `structureSearch` (`operation: files`, `view: paths`) pages the section; each page is a `skeleton` event, so nodes appear before any ingest finishes (`git ls-files` when the tool is unavailable).
2. **Plan**: a directory with `package.json` or `Cargo.toml` is a chunk; files outside every package form loose chunks that exclude the package directories by name. `--whole` plans one chunk.
3. **Graph**: chunks build with `graphBuild`, `--ingest-jobs` at a time (default a quarter of the cores); builds overlap because the engine locks only publication. One slot always holds the largest remaining chunk, which bounds the run, and the others take the smallest so the viewer fills in early. Each build's stderr progress (`parsed N/M files`, `linked`, `published`) updates its chunk row. A chunk the graph refuses with `scopeTooBroad` splits one level and the parts rejoin the queue. Every `graphQuery` call sends `responseChars: 50000` and follows the tool's own continuations (`responsePagination.next`, `next.nextPage`, list and cycle-member pages); each page (files, witnesses, symbols) streams as a `rows` event with root-relative ids. Queries that need nothing else start at once; witnesses, symbols and call gaps are walked as concurrent groups of source files (multi-target `target`, about four groups per `--jobs` worker, default the core count), and each union must equal its whole walk's `pagination.totalItems` with every id once, else the single walk runs. A tree that changes during a build is graphed from a frozen copy under the chunk's `tmp/`.
4. **Stitch**: imports between chunks from out-of-root relative imports and workspace package names; calls bound by exact top-level name through those imports (`stitched`, medium). Call gaps are read only for the files that import another chunk: one graphQuery `unresolved` per group of files (as many as the schema's `target` list allows) with `edges: ["calls"]` and `reasons: ["not-declared-in-repo"]`, filtered before paging; none for a single chunk. Cycles and findings stay per chunk.
5. **LSP**: callees of every function in the hub files and of every stitched call source, batched by the published `lspSearch` limit, continuations followed per query. Graph calls it confirms become `both`, unconfirmed probed ones stay `graph`, callees only the server reports join as `lsp`. A language without a server is reported and skipped; no step has a fixed timeout.
6. **Verify**: every graph node is checked against its file on disk: the file exists, every declaration and edge line falls inside it, and graphQuery `stale` does not report it changed since its chunk's snapshot. Nodes that fail get a warning (stale) or critical (missing, line outside the file) ring and stay listed in Insights.
7. **Metrics**: one weighted file graph (`graph-model.mjs`: w = [imports] + Σ call sites × evidence, evidence 1 when LSP resolved the call, else confidence high 1 / medium 0.5 / low 0.25; test-only edges excluded) measured with standard algorithms (`algorithms.mjs`): Tarjan strongly connected components (cycles across the whole stitched graph, with languages and chunks), dependency depth on the condensation DAG, weighted PageRank to convergence, Brandes betweenness, Louvain communities with modularity, k-core, and Martin package coupling (Ca, Ce, instability). Values land on `files[].metrics`, sets on `metrics`, weighted edges on `fileEdges`; then the briefs, facts only.

## Viewer and server

The viewer can chat with the agent that opened it (click a node, ask; the reply appears in the drawer) and refresh after code changes: `node <skill>/scripts/reply.mjs <dir> "answer" --chat <id> [--refresh]` or the channel's `reply`/`refresh_graph` tools. Details and the security notes: [chat](chat.md).

Event batches travel as gzipped binary WebSocket frames (lossless; a full-repo replay is about 20× smaller), decoded in arrival order by the page. `serve.mjs` binds 127.0.0.1 on a random (or `--port`) port, serves only its own assets, and requires the run's random token in the page URL and the WebSocket URL plus a same-server `Origin`. The pipeline starts with the server; a page opened later (or reconnecting) sends `hello {since}` and receives every missed event. The server stays up after `done` so the page can ask for AST outlines (`astSearch symbols`) and LSP `callers`, `callees` or `references`; those answers are events too.

Events (one JSON object per line in `events.jsonl`, each with `seq` and `t` ms):

| type | Carries |
|---|---|
| `run`, `phase`, `progress` | Root and options; phase start/done/skipped with notes; done/total per phase |
| `skeleton`, `plan`, `chunk` | Listed paths per page; planned chunks; chunk state (ingesting, paging, done, split, error) with counts, and `build`: the latest graphBuild progress line while ingesting |
| `rows` | `files`, `witnesses` or `symbols` rows of one graph page, root-relative |
| `findings` | One chunk's ranked issues, cycles, health, detector counts |
| `stitched`, `lsp` | Cross-chunk imports and calls, package nodes, gaps; per-batch call provenance updates and LSP-only calls |
| `tool` | Every octocode call: tool, detail, page (first or cursor), rows, total, continues, ms, bytes, error |
| `metrics`, `done`, `error`, `note` | Per-file metric columns aligned to one id list, weighted edges as index columns, cycles, communities, packages, timings; result paths; failures; warnings |
| `ast`, `lspResult` | On-demand answers from the page |

## Script owners

| When | Read |
|---|---|
| Entry points | [serve.mjs](../scripts/serve.mjs), [build.mjs](../scripts/build.mjs), [preflight.mjs](../scripts/preflight.mjs) |
| Chat with the agent from the viewer, Claude Code channel | [chat](chat.md), [channel.mjs](../scripts/channel.mjs), [reply.mjs](../scripts/reply.mjs), [tests](../scripts/chat.test.mjs) |
| Ask the graph; answers over the stitched weighted graph | [query](query.md), [query.mjs](../scripts/query.mjs), [query-ops.mjs](../scripts/query-ops.mjs), [query-index.mjs](../scripts/query-index.mjs), [serve-answers.mjs](../scripts/serve-answers.mjs), [tests](../scripts/query.test.mjs) |
| Code wiki (Karpathy LLM Wiki pattern over communities) | [wiki](wiki.md), [wiki.mjs](../scripts/wiki.mjs), [tests](../scripts/wiki.test.mjs) |
| Refresh a result after edits (only changed chunks rebuild) | [refresh.mjs](../scripts/refresh.mjs), [tests](../scripts/refresh.test.mjs) |
| Event pipeline, phases, on-demand AST/LSP | [pipeline.mjs](../scripts/pipeline.mjs) |
| CLI calls, tool-call hook, worker pool | [lib.mjs](../scripts/lib.mjs) |
| One chunk's graph pages and data | [section.mjs](../scripts/section.mjs), [graph-pages.mjs](../scripts/graph-pages.mjs) |
| Chunk boundaries and refinement | [plan.mjs](../scripts/plan.mjs) |
| Joining chunks | [stitch.mjs](../scripts/stitch.mjs) |
| LSP cross-check | [lsp.mjs](../scripts/lsp.mjs) |
| Each node checked against its file | [verify.mjs](../scripts/verify.mjs) |
| Following tool continuations, tool-call summaries | [tools.mjs](../scripts/tools.mjs) |
| Local server and WebSocket | [serve.mjs](../scripts/serve.mjs), [ws.mjs](../scripts/ws.mjs) |
| Viewer | [live.html](../assets/live.html), [live.js](../assets/live.js) (store), [live-graph.js](../assets/live-graph.js) (canvas), [live-panels.js](../assets/live-panels.js) (panels) |
| Weighted graph and its algorithms | [graph-model.mjs](../scripts/graph-model.mjs), [algorithms.mjs](../scripts/algorithms.mjs), [metrics.mjs](../scripts/metrics.mjs), [tests](../scripts/algorithms.test.mjs) |
| Briefs (facts only), model overlay | [render.mjs](../scripts/render.mjs), [model.mjs](../scripts/model.mjs) |
| Tests | `node --test scripts/*.test.mjs`: [live](../scripts/live.test.mjs), [graph pages](../scripts/graph-pages.test.mjs), [stitch](../scripts/stitch.test.mjs), [temp dirs](../scripts/test-tmp.mjs) |
