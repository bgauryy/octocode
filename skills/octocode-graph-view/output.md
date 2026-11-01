# Output

Follow the [shared presentation rules](references/presentation.md). Report BLUF: what the graph supports, deciding evidence, and coverage limits. Keep LSP-only or stitched calls, inferred layers and unresolved leads separate from proven facts. Cite `data.json` fields or `path:line` beside claims; every node and edge has `loc`.

## Saved result

One result directory at `<workspace>/.octocode/octocode-graph-view/<YYYY-MM-DD>-<slug>-<random>/` unless the user names `--out`. Workspace means the target project, never the skill installation.

| Artifact | Consumer and meaning |
|---|---|
| `data.json` | Stitched graph: every file, package, import, function and call with `loc`, each node's file `check`, confidence, provenance (`via`) and every witness line; chunks, coverage, ranked findings, cycles, LSP stats, insights |
| `events.jsonl` | Every event the viewer received, in order (skeleton pages, graph rows, tool calls, LSP batches, AST and LSP answers); `serve.mjs --replay <dir>` reopens it |
| `chunks/<id>/data.json`, `chunks/<id>/tmp/evidence/` | Each chunk's section result and its raw graph pages, gzipped (`*.jsonl.gz`, one JSON line per page; read with `gzip -dc`) |
| `brief.txt`, `brief-full.txt` | Measured facts (counts, top PageRank/betweenness, cycles, communities, package coupling, checks); neither replaces the rows |
| `model.json` (optional) | Authored overlay: [schema](scheme/model.json) |

Report nodes whose file check is not `ok`, unbuilt or split chunks, files in no chunk, unresolved imports between chunks, skipped LSP and its reason, and partial tool results. Graph counts are graph counts.

## Viewer delivery

When a person will look at the graph, run `serve.mjs` and give the printed URL; say it stays live for AST and LSP questions until stopped, and that `--replay` reopens a finished run. Report generation and any browser check separately.
