# Output

Use `loc` on a saved node or edge to cite its source. The `via`, `resolution` and confidence fields show how the tool identified a call.

## Saved result

Builds create a fresh directory under `.octocode/octocode-graph/` in the current working directory. Pass `--out <dir>` to choose the destination.

| Artifact | Consumer and meaning |
|---|---|
| `data.json` | Stitched graph with locations for files, packages, imports, functions and calls. Each entry in the saved `files` array carries its own `check`. Jump from a call to its source using `loc`, `via`, `resolution`, confidence and recorded witness lines. Includes chunks, coverage, findings, cycles, LSP stats and insights |
| `events.jsonl` | Every event the viewer received, in order (skeleton pages, graph rows, tool calls, LSP batches, AST and LSP answers); `serve.mjs --replay <dir>` reopens it |
| `chunks/<id>/data.json`, `chunks/<id>/tmp/evidence/` | Each chunk's section result and its raw graph pages, gzipped (`*.jsonl.gz`, one JSON line per page; read with `gzip -dc`) |
| `brief.txt`, `brief-full.txt` | Measured facts (counts, top PageRank/betweenness, cycles, communities, package coupling, checks); neither replaces the rows |
| `model.json` (optional) | Authored overlay: [schema](scheme/model.json) |

Report the coverage gaps that limit a reported claim, beside that claim: failed snapshot freshness checks (`verification.staleCheck`), files whose `check.status` differs from `ok`, unbuilt or split chunks, files in no chunk, unresolved imports between chunks, skipped LSP and its reason, and partial tool results. Graph counts are graph counts.

## Viewer delivery

For a file delivery, return the result directory or export a standalone page with the package CLI, `octocode-graph html <saved-run> <new.html>`. For live queries and chat, share the URL printed by `serve.mjs`. Use `--replay <result-dir>` to revisit a saved run.
