Load when answering a structural question from a built graph (who depends on a file, the path between two files, a cycle, a community, symbol callers), pointing at nodes in the viewer, or reading and writing the wiki from the shell or the channel.

# Ask the graph

For queries against a native snapshot, use the operation table in the `octocode-graph` skill. The commands here query the viewer graph, including stitched edges and available LSP confirmations. After the `graph-ready` event, ask the live viewer for the latest evidence or pass `<out>/stitched.json` to the shell query for the initial snapshot.

One query language answers over the whole stitched, weighted file graph of a result (`data.json`: every chunk, cross-chunk imports and calls, the weights recorded on file edges). Production edges only unless `--tests`. Walks (`impact`, `deps`, `dependents`, `path`, `cycle`) follow strong edges: an import or a call with full evidence (LSP-resolved or high confidence). Metrics count every production edge, so a walk can be empty for a file the metrics place in a cycle; its summary and `stats.weakAdds` (`weakSize` for `cycle`, `weakHops` for `path`) state what `--weak` would add, which is edges backed only by calls bound by name (graph confidence medium or low). `--imports` walks import edges only. Rows are complete: nothing is cut to a page size; the viewer pages them.

| Query | Answer |
|---|---|
| `search <text> [--files\|--symbols] [--all]` | Files and symbols ranked by how their name matches: `exact` (ignoring case and separators, so `parse_args` finds `parseArgs`), `prefix`, `words` (each word or initial starts a name word: `p args`, `gun` → `getUserName`), `substring`, `terms` (every word in the name or path), `path` (files only), then `fuzzy` (letters in order, tried only when nothing else matches). Ties rank by call sites received, exported, then PageRank. Rows carry `match`, `rank`, `callSites`; the best 20 show and the summary names the `--all` command for every match. Every substring match is among its rows (`--all` lists them all). |
| `about <file\|file:line\|symbol>` | One answer around an entity. A file: its `metrics`, `pagerankRank`, `community`, file check, then `dependency` and `dependent` rows (depth 1, strong edges) and `declares` rows by call sites. A symbol: its declaration, then `caller` and `callee` rows. Each row has a `role`. A name that is not a file resolves as a symbol; ambiguity returns candidates. |
| `region <entry> <outcome>` | Everything that can take part in a route from the entry to the outcome: reachable forward from one and backward from the other. Files by default; two `file#symbol` arguments ask the call graph. Rows carry `fromEntry` and `toOutcome` hops and `mustPass`; `stats.mustPass` lists the steps on every modeled route (the outcome's dominators). |
| `path <a> <b> --avoid <file\|dir>...` | An architecture rule as a path: a route that passes none of the avoided files (a directory avoids every file under it) is a violation of a rule that every route must cross them, with each hop's line; none means the rule holds. |
| `critical functions [n\|all]` | Functions ranked by PageRank over the call graph weighted by call sites × evidence, each with `callSites`, `callers`, `callerFiles` and `transitiveCallers`. The best 20 show; the summary names the `all` form. |
| `critical points [n\|all]` | Files whose removal disconnects the production file graph (articulation points): `value` is the files cut off from the rest of the component, `parts` the sizes left. |
| `critical path [<file>] [--up]` | The longest dependency chain below a file (with `--up`, the longest chain of dependents above it); without a file, the deepest chain in the graph. Rows carry each hop's weight, import lines and calls, like `path`. |
| `files <file> [file ...]` | Saved file records and source locations in input order. JSON includes `sourceQueries` for native `localFetch`. Plain paths request the whole source file. A `path:line` requests that line. Multiple lines in one file share one source query. |
| `impact <file...> [depth N]` | Every file that transitively depends on it (reverse breadth-first), each with its depth, the heaviest edge that reached it (`weight`, `through`) and that edge's line; counts per depth and per chunk |
| `deps <file...> [depth N]` / `dependents <file...> [depth N]` | Forward / reverse neighbors; adjacent by default (`depth all` walks everything) |
| `path <a> <b> [--hops]` | The strongest dependency path a → b (Dijkstra, cost Σ 1/weight); add `--hops` for a minimum-hop path on the same edges (BFS). Both return the weight, import lines and symbol calls at each hop. In the viewer, **Compare fewest hops** reruns the weighted query with its filters. No path: says so and checks b → a |
| `order <file>` | Prerequisite-first order for the seed and its reachable dependencies. Files in a strongly connected component share a group. `crossings` contains the evidence for every edge between groups. Add `--imports` for an import-only order. |
| `trace <seed> <pattern>` | Match typed walks from a file, symbol, or provider entity. Juxtaposition or `·` means sequence, `\|` means choice, `*` includes zero steps and `+` requires at least one. Parentheses group expressions. Add `--depth N` to bound the whole pattern to N relationships, including zero. Example: `trace src/main.ts DEFINES (CALLS)+`. Native export relations are `DEFINES`, `IMPORTS` and `CALLS`. JSON returns each matching endpoint and every witness on an accepting walk. [Additional provider relations](providers.md) |
| `cycle [<file>]` | With a file: its strongly connected set under the selected filters, with its internal weight and one shortest ring through the file, starting at the file; among rings of equal length, the next hop with the smallest id wins. Other members follow in PageRank order. A `metrics.cycles` ID identifies an exact match of the member set. With no file: every cycle the build measured, largest first, then by smallest member, each with all its members. A file that imports itself is a one-member cycle. |
| `community <file\|id>` | Members by PageRank and the weight to every other community (`links`, out and in) |
| `callers <sym>` / `callees <sym>` | Symbol-level calls with the saved `via` value, confidence and lines. Look in `data.json` for `resolution: ["stitched-import"]` to identify calls bound by name across chunks. `via: scip` marks a Rust call that a rust-analyzer SCIP index resolved; that index runs only if the user's own rust-analyzer settings enable build scripts and proc macros. Use `file#name` (any file form) or a bare symbol name. |
| `links [<file>] [--unresolved\|--stale] [--rule <rule>]` | Links beside the code graph, each with its citing `path:line`: docs (`md-link`, `md-path`, `md-cite`, `md-symbol`), manifests and tsconfigs (`manifest`, `tsconfig`) and agents (`agent`, with `confidence` and `proposedBy`). `--unresolved` lists the references that named no single target with `reason` and `candidates`; `--stale` the citations whose code changed under an unchanged doc line. `about` adds a file's or symbol's `documentedBy` rows. [Links](links.md) |
| `top <metric> [n\|all]` | Files ranked by `pagerank`, `betweenness`, `inWeight`, `outWeight`, `core` or `depth`; use `all` for the full ranking and check the summary's coverage |

For a change spanning several files, pass the paths to one `impact`, `deps` or `dependents` query. Each reached file appears once, at its minimum depth from any input. Seeds stay in the highlight and are excluded from result rows. Ask separate queries when you need attribution for every seed.

For several independent questions, keep one saved snapshot open. Blank input lines are ignored. Each other line produces a complete result. A failed question sets a nonzero exit status while later questions still run.

```sh
printf '%s\n' 'files src/main.ts:18 src/lib.ts:42' 'impact src/main.ts src/lib.ts depth 2' 'top pagerank 3' |
  octocode-graph query ./saved-run --stdin --json
```

For known source paths, ask `localFetch` directly. Batch several paths in its `queries` array and combine ranges from one file in a single item. To resolve unfamiliar filenames first, use the `sourceQueries` returned by `files`, grouped up to the limit in the local tool schema. Source reads reflect the current checkout. Follow its continuation to retrieve remaining lines.

A file is a root-relative path, an absolute path under the root, `path:line`, a unique suffix (`pipeline.ts`) or a node id. An ambiguous or unknown argument is an error with every candidate as a re-runnable query (`impact b/util.ts`, `impact c/util.ts`); nothing is guessed.

Queries may be written naturally: filler words (`callers of x`, `path from a to b`), op typos and synonyms (`serach`, `cycles`, `dependencies`, `who calls x`), metric nicknames (`top pr`, `top in`), a whole query in quotes, and a misspelled file or symbol with one near match are each read as one exact query. The result's `readAs` gives that query and each change, and the text output starts with `read as:`. When a misspelling or a typo is near several names, nothing is answered and the candidates come back instead.

A `callers` answer is complete only when every callable was probed and every probe answered (`--lsp all`). Any other `callers` answer is `isPartial` with `partialReasons: ["callersNotProbed"]`, carries `warnings` (what the graph cannot see: calls from other chunks are bound only through stitched imports and the build's LSP probes; calls inside Rust macros reach only the language server) and `confirm`, a ready `lspSearch` call for the same callers. Run it before claiming a complete caller list.

Results include `scope` with the export root, snapshot label and known coverage status. A null coverage status means the export did not record it. `nodes` are the ids to highlight, `edges` the `[u, v]` pairs that carry the answer, each row `{ id, label, chunk, loc: { path, line }, ... }` plus its op's fields (`depth`, `weight`, `through`, `nextFile`, `order`, `rank`, `value`, `symbol`, `via`, `evidence`).

## Where to ask

| From | How |
|---|---|
| Viewer | The bar in the graph toolbar: Enter asks, ↑/↓ walks the history, a row selects its file, candidates and linked communities re-run, Show / Hide (Esc) / Clear the highlight |
| Library | `openGraph(dir).query(text)` from `@octocodeai/octocode-graph` returns the same result objects |
| Shell, running viewer | `octocode-graph reply query <out> "impact pipeline.ts"` prints the JSON result; the viewer highlights it. Exit 1 with the JSON when the query fails or is ambiguous |
| Shell, no viewer | `octocode-graph query <out> "path a.ts b.ts" [--json]` reads `data.json` directly (text, or JSON with `--json`), or asks the run's live viewer when one serves it (it holds the graph loaded). Pass a project directory instead of `<out>` to use its newest saved graph under `.octocode/octocode-graph/` (of the current directory or the project); stderr names the run. Every answer, on the CLI, over MCP and in the viewer, warns of graph files changed since the graph was written and lists them all in `staleFiles` |
| Saved graph MCP | `octocode-graph mcp`: `query_graph({ result, query })` answers from a saved result (or a project directory's newest one): text by default, `format: "json"` for every field. `build_graph`, `refresh_graph({ result })`, `diff_graph({ result })` and `link_graph` complete the loop; the engine's `graphBuild` and `graphQuery` are on the same server |
| Viewer channel | `query_graph({ query })` answers the open viewer in the same text and highlights the answer |

Every answer from the viewer, the shell or the channel is a `query` event (`{ type: 'query', id: 'q…', from: 'viewer'\|'agent', ...result }`), saved in `events.jsonl` also in a replay; a replay restores the bar's history without moving the camera. The server answers from `data.json` in memory and reloads it when a run or refresh rewrites it; during the first build a query says the graph is still building.

## Point at the graph

A reply can name the files it is about: `octocode-graph reply <out> "text" --chat <id> --nodes a.ts,src/b.ts` or the channel's `reply({ chat_id, text, nodes })`. Nodes resolve like query arguments before anything is sent; an unknown or ambiguous one refuses the reply and lists candidates. The newest reply with nodes lights them up and frames them; every such reply keeps a **Show on graph (N)** action.

## Wiki

Read, write, promote or lint community pages through `octocode-graph reply wiki <out> …` or the channel's `wiki_*` tools; [wiki operations](wiki.md#operations) owns the commands. `promote --chat <id>` takes the latest reply and, without a slug, the page holding the question's file. Writes update the viewer. `POST /query` and `/wiki` require the run token in `?t=` and refuse a foreign `Origin`.
