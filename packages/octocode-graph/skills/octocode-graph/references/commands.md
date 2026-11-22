Load when building, serving or replaying a graph, reading events, or changing a script.

# Commands

Requires Node 24.15+ and a browser for the viewer; `rsync` supports the frozen-copy fallback. The viewer serves its [bundled WebGL renderer](../assets/renderer.generated.js) and [third-party licenses](../assets/renderer.generated.LICENSE.txt) from the local server. `preflight.mjs` reports available layers.

Pass a file to build or serve a graph of that file alone. Pass its enclosing directory when you need cross-file imports and calls. Run from the target workspace, which defines paths and allowed roots. Each script's `--help` owns its flags. The graph package gets its engine from its `octocode` dependency. A standalone skill can use `octocode` on PATH. Set `OCTOCODE_CLI` to select a specific engine. The build and viewer validate options before starting work. In a channel with multiple viewers, each result directory records the tool calls for its own build and refreshes. Use a new output directory for each build and `--replay` to reopen a saved run. The selected CLI’s full catalog identifies the executable and each tool’s batch limit. The pipeline freezes that executable in a private directory under the OS temp directory, keyed by its SHA-256 digest. Runs can reuse the same image and warm language servers. An incompatible schema stops the build before file listing.

| Goal | Command |
|---|---|
| What runs here | `node <skill>/scripts/preflight.mjs [--json]` |
| Live viewer | `node <skill>/scripts/serve.mjs <section> [--out <dir>] [--whole] [--lsp on\|off] [--jobs N] [--ingest-jobs N] [--exclude a,b] [--port N] [--no-open] [--exit-when-done]` |
| Headless result | `node <skill>/scripts/build.mjs <section> [same flags] [--progress]` |
| Reopen a finished run | `node <skill>/scripts/serve.mjs --replay <result-dir>` |
| Briefs again (after `model.json`) | `node <skill>/scripts/render.mjs --data <dir>/data.json` |

## Pipeline

1. **Files**: `structureSearch` (`operation: files`, `view: paths`) pages the section; each page is a `skeleton` event, so nodes appear before any ingest finishes (`git ls-files` when the tool is unavailable).
2. **Plan** package and loose chunks from the listed files whose extensions the CLI advertises for graph facts. Source-bearing fixtures participate. The skeleton keeps the full listing. Use `--whole` for one native snapshot over the section.
3. **Graph**: chunks build with `graphBuild`, with concurrency controlled by `--ingest-jobs`; `--jobs` controls query workers. Stderr progress (`parsed N/M files`, `linked`, `published`) updates each chunk. Paused builds follow `next.expandScan` unchanged until a snapshot is published. A `scopeTooBroad` refusal splits that chunk and queues its parts. The reply's `next` is followed to its end and `rowPart` fragments reassemble before evidence lists or cycle-member pages; rows stream once with root-relative IDs. Independent operations share a call up to the published batch limit. Each query keeps its result index across pages and follows its own evidence branches. When a chunk has more files than a source-target list can hold, the viewer uses its complete native file inventory to partition witnesses and symbols among query workers. Each file belongs to one group, and the worker follows each issued continuation in order. A partial inventory falls back to the full snapshot walk. A witness walk preserves separate occurrences with identical displayed fields. A tree that changes during a build is graphed from a frozen copy under the chunk's `tmp/`.
4. **Stitch**: imports between chunks from out-of-root relative imports, Rust paths into other workspace crates (`octocode_io::memo::Memo` to that crate's deepest module file; the crate name is the Cargo `[lib] name`, else the package name) and workspace package names; calls bound by exact top-level name through those imports or by a workspace crate path to its module file (`stitched`, medium). Call gaps are read only for the files that import another chunk: one graphQuery `unresolved` per group of files (as many as the schema's `target` list allows) with `edges: ["calls"]` and `reasons: ["not-declared-in-repo", "external-package"]`, filtered before paging; none for a single chunk. Cycles and findings stay per chunk.
During a large build the live map groups files by folder. Folder labels show parsed files over listed files. Click a folder to open it, or search for a file to jump directly to it. When metrics arrive the overview switches to dependency communities. The Folders or Communities button switches between grouped and individual files.

After stitching the pipeline atomically publishes `stitched.json` and emits `graph-ready`. The live query endpoint reads the current graph, including completed LSP batches. Each answer labels pending confirmation until the LSP phase finishes. The final `data.json` is published atomically after verification and metrics.

5. **LSP**: callees of every function in the hub files and of every stitched call source, batched by the published `lspSearch` limit, continuations followed per query. Graph calls it confirms become `both`, unconfirmed probed ones stay `graph`, callees only the server reports join as `lsp`. A language without a server is reported and skipped; no step has a fixed timeout. When the only gap is `locationsWithheld`, a returned definition can confirm a call by file, name and declaration line. The probe stays incomplete. Each batch streams newly incomplete probes, and `callStats.unanswered` in the final graph lists all of them.
6. **Verify** each file against its declarations, recorded edge lines and the snapshot. For status and line problems, read the `check` object on each saved file. Freshness queries share batches up to the native limit. A failed batch is retried by snapshot. Files with unknown freshness are marked `unverified` unless a disk or line check has already found a problem. Insights shows affected files and the reason.
7. **Metrics**: the production file graph weights imports and call sites by binding evidence; [graph-model.mjs](../scripts/graph-model.mjs) owns that formula. [algorithms.mjs](../scripts/algorithms.mjs) computes cycles, dependency depth, PageRank, betweenness, communities, k-core and package coupling. Values land on `files[].metrics`, sets on `metrics`, weighted edges on `fileEdges`, then in the briefs. They measure structure, not runtime frequency.

## Viewer and server

The viewer can chat with the agent that opened it (click a node, ask; the reply appears in the drawer) and refresh after code changes: `node <skill>/scripts/reply.mjs <dir> "answer" --chat <id> [--refresh]` or the channel's `reply`/`refresh_graph` tools. Details and the security notes: [chat](chat.md).

Event batches travel as lossless gzipped WebSocket frames, decoded in arrival order. `serve.mjs` binds 127.0.0.1 on a random (or `--port`) port, serves only its own assets, and requires the run token in the page and WebSocket URLs plus a same-server `Origin`. The pipeline starts with the server; a reconnecting page sends `hello {since}` to recover missed events. The server remains available after `done` for AST outlines (`astSearch symbols`) and LSP `callers`, `callees` or `references`; those answers are events too.

Each line of `events.jsonl` records an event with a sequence number and elapsed milliseconds. Count distinct tool IDs or start events for call volume. Use terminal events for summed tool time and phase boundaries for wall time.

| type | Carries |
|---|---|
| `run`, `phase`, `progress` | Root and options; phase start/done/skipped with notes; done/total per phase |
| `skeleton`, `plan`, `chunk` | Listed paths per page; planned chunks; chunk state (ingesting, paging, done, split, error) with counts, and `build`: the latest graphBuild progress line while ingesting |
| `rows` | `files`, `witnesses` or `symbols` rows of one graph page, root-relative |
| `findings` | One chunk's ranked issues, cycles, health, detector counts |
| `stitched`, `graph-ready`, `lsp` | Cross-chunk imports and calls, package nodes, gaps; per-batch call provenance updates and LSP-only calls |
| `tool` | Every octocode call: tool, detail, page (first or cursor), rows, total, continues, ms, bytes, error |
| `metrics`, `done`, `error`, `note` | Per-file metric columns aligned to one id list, weighted edges as index columns, cycles, communities, packages, timings; result paths; failures; warnings |
| `ast`, `lspResult` | On-demand answers from the page |

## Script owners

| When | Read |
|---|---|
| Entry points and installed symlinks | [serve.mjs](../scripts/serve.mjs), [build.mjs](../scripts/build.mjs), [preflight.mjs](../scripts/preflight.mjs), [shared entry-point check](../scripts/lib/entrypoint.mjs) |
| Chat with the agent from the viewer, Claude Code channel | [chat](chat.md), [channel.mjs](../scripts/channel.mjs), [reply.mjs](../scripts/reply.mjs) |
| Ask the graph; answers over the stitched weighted graph | [query](query.md), [query.mjs](../scripts/query.mjs), [query-ops.mjs](../scripts/query-ops.mjs), [query-index.mjs](../scripts/query-index.mjs), [serve-answers.mjs](../scripts/serve-answers.mjs) |
| Code wiki (Karpathy LLM Wiki pattern over communities) | [wiki](wiki.md), [wiki.mjs](../scripts/wiki.mjs) |
| Refresh a result after edits (only changed chunks rebuild) | [refresh.mjs](../scripts/refresh.mjs) |
| Event pipeline, phases, on-demand AST/LSP | [pipeline.mjs](../scripts/pipeline.mjs) |
| CLI calls, tool-call hook, worker pool | [lib.mjs](../scripts/lib.mjs) |
| One chunk's batched graph pages and data | [section.mjs](../scripts/section.mjs), [graph-pages.mjs](../scripts/graph-pages.mjs) |
| Chunk boundaries and refinement | [plan.mjs](../scripts/plan.mjs) |
| Joining chunks and preserving partial coverage | [stitch.mjs](../scripts/stitch.mjs), [coverage.mjs](../scripts/coverage.mjs) |
| LSP cross-check | [lsp.mjs](../scripts/lsp.mjs) |
| Each node checked against its file | [verify.mjs](../scripts/verify.mjs) |
| Following tool continuations, tool-call summaries | [tools.mjs](../scripts/tools.mjs) |
| Local server and WebSocket | [serve.mjs](../scripts/serve.mjs), [ws.mjs](../scripts/ws.mjs) |
| Viewer | [live.html](../assets/live.html), [live.js](../assets/live.js) (store), [live-graph.js](../assets/live-graph.js) (canvas), [live-panels.js](../assets/live-panels.js) (panels) |
| Weighted graph and its algorithms | [graph-model.mjs](../scripts/graph-model.mjs), [algorithms.mjs](../scripts/algorithms.mjs), [metrics.mjs](../scripts/metrics.mjs) |
| Briefs (facts only), model overlay | [render.mjs](../scripts/render.mjs), [model.mjs](../scripts/model.mjs) |

A viewer connection resumes after its last applied sequence number. Replay and live traffic use ordered gzip batches sized to the socket write target. A larger event travels whole. A slow connection resumes as its socket drains.

For a standalone HTML file, use the installed package command `octocode-graph html <result-dir> <new.html>` after the build finishes. The file embeds the saved events, graph evidence, renderer and query engine. New AST and language-server requests require a live viewer.
