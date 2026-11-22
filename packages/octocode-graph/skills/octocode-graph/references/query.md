Load when answering a structural question from a built graph (who depends on a file, the path between two files, a cycle, a community, symbol callers), pointing at nodes in the viewer, or reading and writing the wiki from the shell or the channel.

# Ask the graph

For queries against a native snapshot, use the operation table in the `octocode-graph` skill. The commands here query the viewer graph, including stitched edges and available LSP confirmations. After the `graph-ready` event, ask the live viewer for the latest evidence or pass `<out>/stitched.json` to the shell query for the initial snapshot.

One query language answers over the whole stitched, weighted file graph of a result (`data.json`: every chunk, cross-chunk imports and calls, the weight rule of [graph-model.mjs](../scripts/graph-model.mjs)). Production edges only unless `--tests`. Walks (`impact`, `deps`, `dependents`, `path`, `cycle`) follow strong edges: an import or a call with full evidence (LSP-resolved or high confidence). Metrics count every production edge, so a walk can be empty for a file the metrics place in a cycle; its summary and `stats.weakAdds` (`weakSize` for `cycle`, `weakHops` for `path`) state what `--weak` would add, which is edges backed only by calls bound by name (graph confidence medium or low). `--imports` walks import edges only. Rows are complete: nothing is cut to a page size; the viewer pages them.

| Query | Answer |
|---|---|
| `impact <file> [depth N]` | Every file that transitively depends on it (reverse breadth-first), each with its depth, the heaviest edge that reached it (`weight`, `through`) and that edge's line; counts per depth and per chunk |
| `deps <file> [depth N]` / `dependents <file> [depth N]` | Forward / reverse neighbors; adjacent by default (`depth all` walks everything) |
| `path <a> <b>` | The strongest dependency path a → b (Dijkstra, cost Σ 1/weight); every hop with its weight, import lines and symbol calls. No path: says so and checks b → a |
| `cycle <file>` | Its strongly connected set (the `metrics.cycles` id when production), first the files of one shortest ring through it in order, then the rest by PageRank |
| `community <file\|id>` | Members by PageRank and the weight to every other community (`links`, out and in) |
| `callers <sym>` / `callees <sym>` | Symbol-level calls with the saved `via` value, confidence and lines. Look in `data.json` for `resolution: ["stitched-import"]` to identify calls bound by name across chunks. Use `file#name` (any file form) or a bare symbol name. |
| `top <metric> [n\|all]` | Files ranked by `pagerank`, `betweenness`, `inWeight`, `outWeight`, `core` or `depth`; use `all` for the full ranking and check the summary's coverage |
| `find <text>` | Files whose path and symbols whose name contain the text |

A file is a root-relative path, an absolute path under the root, `path:line`, a unique suffix (`pipeline.mjs`) or a node id. An ambiguous or unknown argument is an error with every candidate as a re-runnable query (`impact b/util.mjs`, `impact c/util.mjs`); nothing is guessed.

Result: `{ ok, op, text, error?, candidates?, nodes, edges, rows, summary, stats }`. `nodes` are the ids to highlight, `edges` the `[u, v]` pairs that carry the answer, each row `{ id, label, chunk, loc: { path, line }, ... }` plus its op's fields (`depth`, `weight`, `through`, `next`, `order`, `rank`, `value`, `symbol`, `via`, `evidence`).

## Where to ask

| From | How |
|---|---|
| Viewer | The bar in the graph toolbar: Enter asks, ↑/↓ walks the history, a row selects its file, candidates and linked communities re-run, Show / Hide (Esc) / Clear the highlight |
| Shell, running viewer | `node <skill>/scripts/reply.mjs query <out> "impact pipeline.mjs"` prints the JSON result; the viewer highlights it. Exit 1 with the JSON when the query fails or is ambiguous |
| Shell, no viewer | `node <skill>/scripts/query.mjs <out> "path a.ts b.ts" [--json]` reads `data.json` directly (text, or JSON with `--json`) |
| Channel | `query_graph({ query })` returns every row as `path:line` text and highlights the answer |

Every answer from the viewer, the shell or the channel is a `query` event (`{ type: 'query', id: 'q…', from: 'viewer'\|'agent', ...result }`), saved in `events.jsonl` also in a replay; a replay restores the bar's history without moving the camera. The server answers from `data.json` in memory and reloads it when a run or refresh rewrites it; during the first build a query says the graph is still building.

## Point at the graph

A reply can name the files it is about: `reply.mjs <out> "text" --chat <id> --nodes a.ts,src/b.ts` or the channel's `reply({ chat_id, text, nodes })`. Nodes resolve like query arguments before anything is sent; an unknown or ambiguous one refuses the reply and lists candidates. The newest reply with nodes lights them up and frames them; every such reply keeps a **Show on graph (N)** action.

## Wiki

Read, write, promote or lint community pages through `reply.mjs wiki <out> …` or the channel's `wiki_*` tools; [wiki operations](wiki.md#operations) owns the commands. `promote --chat <id>` takes the latest reply and, without a slug, the page holding the question's file. Writes update the viewer. Missing `scripts/wiki.mjs` returns HTTP 501, "wiki unavailable". `POST /query` and `/wiki` require the run token in `?t=` and refuse a foreign `Origin`.

Owners: [query.mjs](../scripts/query.mjs) (language, CLI), [query-index.mjs](../scripts/query-index.mjs) (graph index, argument resolution), [query-ops.mjs](../scripts/query-ops.mjs) (answers), [serve-answers.mjs](../scripts/serve-answers.mjs) (server: data in memory, `/query`, `/wiki`, reply nodes), [live-query.js](../assets/live-query.js) (bar).
