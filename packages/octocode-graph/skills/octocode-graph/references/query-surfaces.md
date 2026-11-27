Load when batching questions, or asking from the viewer, the library, the shell, MCP or the channel.

# Where to ask a saved graph

## Several questions in one load

For several independent questions, keep one saved snapshot open. Blank input lines are ignored. Each other line produces a complete result. A failed question sets a nonzero exit status while later questions still run.

```sh
printf '%s\n' 'files src/main.ts:18 src/lib.ts:42' 'impact src/main.ts src/lib.ts depth 2' 'top pagerank 3' |
  octocode-graph query ./saved-run --stdin --json
```

## Where to ask

| From | How |
|---|---|
| Viewer | The bar in the graph toolbar: Enter asks, ↑/↓ walks the history, a row selects its file, candidates and linked communities re-run, Show / Hide (Esc) / Clear the highlight |
| Library | `openGraph(dir).query(text)` from `@octocodeai/octocode-graph` returns the same result objects |
| Shell, running viewer | `octocode-graph reply query <out> "impact pipeline.ts"` prints the JSON result; the viewer highlights it. Exit 1 with the JSON when the query fails or is ambiguous |
| Shell, no viewer | `octocode-graph query <out> "path a.ts b.ts" [--json]` reads `data.json` directly (text, or JSON with `--json`), or asks the run's live viewer when one serves it (it holds the graph loaded). A project directory in place of `<out>` answers from its newest saved graph ([resolution, `--refresh`, exit codes](commands.md)); stderr names the run. Every answer, on the CLI, over MCP and in the viewer, warns of graph files changed since the graph was written and lists them all in `staleFiles` |
| Saved graph MCP | `octocode-graph mcp`: `query_graph({ result, query })` answers from a saved result (or a project directory's newest one): text by default, `format: "json"` for every field. `build_graph`, `refresh_graph({ result })`, `diff_graph({ result })` and `link_graph` complete the loop; the engine's `graphBuild` and `graphQuery` are on the same server |
| Viewer channel | `query_graph({ query })` answers the open viewer in the same text and highlights the answer |

Every answer from the viewer, the shell or the channel is a `query` event (`{ type: 'query', id: 'q…', from: 'viewer'\|'agent', ...result }`), saved in `events.jsonl` also in a replay; a replay restores the bar's history without moving the camera. The server answers from `data.json` in memory and reloads it when a run or refresh rewrites it; during the first build a query says the graph is still building.
