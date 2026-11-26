Load when reading a saved result's files, quoting its coverage lines, or delivering a result or the viewer to someone.

# Output

Every path in a saved graph is relative to its root, `data.json` `section`. Query rows add `loc: {path, line}`; an answer's `read:` and `confirm:` lines are the next calls, with absolute paths (`localFetch` the cited range, `lspSearch` callers).

## Saved result

Builds create a fresh directory under `.octocode/octocode-graph/` in the current working directory. Pass `--out <dir>` to choose the destination.

| Artifact | Consumer and meaning |
|---|---|
| `brief.txt` | Read first: counts, top PageRank, betweenness and in-weight, communities, cycles, package coupling, checks, chunks. Facts to orient, not citations |
| `data.json` | The stitched graph, each fact once: `files[]` (with `check` and `metrics`), `symbols[]`, `imports[]` and `calls[]` (lines, confidence, `resolution`, `via`, `crossChunk`), the weighted `fileEdges[]`, `metrics`, `coverage`, `verification`, `callStats`. Ask it with `octocode-graph query`; `jq` reads it too |
| `previous.json` | The graph the last refresh replaced (a hard link until it differs): `octocode-graph diff <dir>` |
| `events.jsonl` | Every event the viewer received, in order; `octocode-graph serve --replay <dir>` reopens it |
| `chunks/<id>/data.json.gz`, `chunks/<id>/evidence/*.jsonl.gz` | Each chunk's section result (refresh reuses it) and its raw graph pages, one JSON line per page; read with `gzip -dc` |
| `model.json` (optional) | Authored overlay: schema exported as `@octocodeai/octocode-graph/model-schema` |

Report the coverage gaps that limit a reported claim, beside that claim. `render` writes them into `brief.txt` as the `COVERAGE`, `FILES` and `CALLS` lines: quote those lines instead of recounting `data.json`. They cover failed snapshot freshness checks (`verification.staleCheck`), files whose `check.status` differs from `ok`, unbuilt or split chunks, files in no chunk, unresolved imports between chunks, skipped LSP and its reason, and partial tool results. Label a count read from the graph as a graph count.

## Viewer delivery

For a file delivery, return the result directory or, after the build finishes, export a standalone page with the package CLI, `octocode-graph html <saved-run> <new.html>`. The page embeds the saved events, graph evidence, renderer and query engine; new AST and language-server requests need a live viewer. For live queries and chat, share the URL printed by `octocode-graph serve`. Use `--replay <result-dir>` to revisit a saved run.
