Load when answering a structural question from a built graph (who depends on a file, the path between two files, a cycle, a community, symbol callers), pointing at nodes in the viewer, or reading and writing the wiki from the shell or the channel.

# Ask the graph

For queries against a native snapshot, use the operation table in the `octocode-graph` skill. The commands here query the viewer graph, including stitched edges and available LSP confirmations. After the `graph-ready` event, ask the live viewer for the latest evidence or pass `<out>/stitched.json` to the shell query for the initial snapshot.

One query language answers over the whole stitched, weighted file graph of a result (`data.json`: every chunk, cross-chunk imports and calls, the weights recorded on file edges). Production edges only unless `--tests`. Walks (`impact`, `deps`, `dependents`, `path`, `cycle`) follow strong edges: an import or a call with full evidence (LSP-resolved or high confidence). Metrics count every production edge, so a walk can be empty for a file the metrics place in a cycle; its summary and `stats.weakAdds` (`weakSize` for `cycle`, `weakHops` for `path`) state what `--weak` would add, which is edges backed only by calls bound by name (graph confidence medium or low). `--imports` walks import edges only. Rows are complete: nothing is cut to a page size; the viewer pages them.

| Query | Answer |
|---|---|
| `files <file> [file ...]` | Saved file records and source locations in input order. JSON includes `sourceQueries` for native `localFetch`. Plain paths request the whole source file. A `path:line` requests that line. Multiple lines in one file share one source query. |
| `impact <file...> [depth N]` | Every file that transitively depends on it (reverse breadth-first), each with its depth, the heaviest edge that reached it (`weight`, `through`) and that edge's line; counts per depth and per chunk |
| `deps <file...> [depth N]` / `dependents <file...> [depth N]` | Forward / reverse neighbors; adjacent by default (`depth all` walks everything) |
| `path <a> <b> [--hops]` | The strongest dependency path a → b (Dijkstra, cost Σ 1/weight); add `--hops` for a minimum-hop path on the same edges (BFS). Both return the weight, import lines and symbol calls at each hop. In the viewer, **Compare fewest hops** reruns the weighted query with its filters. No path: says so and checks b → a |
| `order <file>` | Prerequisite-first order for the seed and its reachable dependencies. Files in a strongly connected component share a group. `crossings` contains the evidence for every edge between groups. Add `--imports` for an import-only order. |
| `trace <seed> <pattern>` | Match typed walks from a file, symbol, or provider entity. Juxtaposition or `·` means sequence, `\|` means choice, `*` includes zero steps and `+` requires at least one. Parentheses group expressions. Add `--depth N` to bound the whole pattern to N relationships, including zero. Example: `trace src/main.ts DEFINES (CALLS)+`. Native export relations are `DEFINES`, `IMPORTS` and `CALLS`. JSON returns each matching endpoint and every witness on an accepting walk. [Additional provider relations](providers.md) |
| `cycle <file>` | Its strongly connected set under the selected filters, with its internal weight and one shortest ring through the file. Other members follow in PageRank order. A `metrics.cycles` ID identifies an exact match of the member set. |
| `community <file\|id>` | Members by PageRank and the weight to every other community (`links`, out and in) |
| `callers <sym>` / `callees <sym>` | Symbol-level calls with the saved `via` value, confidence and lines. Look in `data.json` for `resolution: ["stitched-import"]` to identify calls bound by name across chunks. Use `file#name` (any file form) or a bare symbol name. |
| `top <metric> [n\|all]` | Files ranked by `pagerank`, `betweenness`, `inWeight`, `outWeight`, `core` or `depth`; use `all` for the full ranking and check the summary's coverage |
| `find <text>` | Files whose path and symbols whose name contain the text |

For a change spanning several files, pass the paths to one `impact`, `deps` or `dependents` query. Each reached file appears once, at its minimum depth from any input. Seeds stay in the highlight and are excluded from result rows. Ask separate queries when you need attribution for every seed.

For several independent questions, keep one saved snapshot open. Blank input lines are ignored. Each other line produces a complete result. A failed question sets a nonzero exit status while later questions still run.

```sh
printf '%s\n' 'files src/main.ts:18 src/lib.ts:42' 'impact src/main.ts src/lib.ts depth 2' 'top pagerank 3' |
  octocode-graph-query ./saved-run --stdin --json
```

For known source paths, ask `localFetch` directly. Batch several paths in its `queries` array and combine ranges from one file in a single item. To resolve unfamiliar filenames first, use the `sourceQueries` returned by `files`, grouped up to the limit in the local tool schema. Source reads reflect the current checkout. Follow its continuation to retrieve remaining lines.

A file is a root-relative path, an absolute path under the root, `path:line`, a unique suffix (`pipeline.mjs`) or a node id. An ambiguous or unknown argument is an error with every candidate as a re-runnable query (`impact b/util.mjs`, `impact c/util.mjs`); nothing is guessed.

Results include `scope` with the export root, snapshot label and known coverage status. A null coverage status means the export did not record it. `nodes` are the ids to highlight, `edges` the `[u, v]` pairs that carry the answer, each row `{ id, label, chunk, loc: { path, line }, ... }` plus its op's fields (`depth`, `weight`, `through`, `next`, `order`, `rank`, `value`, `symbol`, `via`, `evidence`).

## Where to ask

| From | How |
|---|---|
| Viewer | The bar in the graph toolbar: Enter asks, ↑/↓ walks the history, a row selects its file, candidates and linked communities re-run, Show / Hide (Esc) / Clear the highlight |
| Shell, running viewer | `octocode-graph reply query <out> "impact pipeline.mjs"` prints the JSON result; the viewer highlights it. Exit 1 with the JSON when the query fails or is ambiguous |
| Shell, no viewer | `octocode-graph query <out> "path a.ts b.ts" [--json]` reads `data.json` directly (text, or JSON with `--json`) |
| Saved graph MCP | `query_graph({ result, query })` reads a saved result directory directly and returns JSON. |
| Viewer channel | `query_graph({ query })` returns every row as `path:line` text and highlights the answer |

Every answer from the viewer, the shell or the channel is a `query` event (`{ type: 'query', id: 'q…', from: 'viewer'\|'agent', ...result }`), saved in `events.jsonl` also in a replay; a replay restores the bar's history without moving the camera. The server answers from `data.json` in memory and reloads it when a run or refresh rewrites it; during the first build a query says the graph is still building.

## Point at the graph

A reply can name the files it is about: `octocode-graph reply <out> "text" --chat <id> --nodes a.ts,src/b.ts` or the channel's `reply({ chat_id, text, nodes })`. Nodes resolve like query arguments before anything is sent; an unknown or ambiguous one refuses the reply and lists candidates. The newest reply with nodes lights them up and frames them; every such reply keeps a **Show on graph (N)** action.

## Wiki

Read, write, promote or lint community pages through `octocode-graph reply wiki <out> …` or the channel's `wiki_*` tools; [wiki operations](wiki.md#operations) owns the commands. `promote --chat <id>` takes the latest reply and, without a slug, the page holding the question's file. Writes update the viewer. `POST /query` and `/wiki` require the run token in `?t=` and refuse a foreign `Origin`.
