Load when asking a saved graph a structural question: owns the walk flags, argument forms and result fields, and routes to each query's row.

# Ask the graph

For queries against a native snapshot, use [native graph queries](operations.md). The commands here query the viewer graph, including stitched edges and available LSP confirmations. After the `graph-ready` event, ask the live viewer for the latest evidence or pass `<out>/stitched.json` to the shell query for the initial snapshot.

One query language answers over the whole stitched, weighted file graph of a result (`data.json`: every chunk, cross-chunk imports and calls, the weights recorded on file edges). Production edges only unless `--tests`. Walks (`impact`, `deps`, `dependents`, `path`, `cycle`, `critical path`) follow strong edges: an import or a call with full evidence (LSP-resolved or high confidence). Metrics count every production edge, so a walk can be empty for a file the metrics place in a cycle; its summary and `stats.weakAdds` (`weakSize` for `cycle`, `weakHops` for `path`) state what `--weak` would add, which is edges backed only by calls bound by name (graph confidence medium or low). `--imports` walks import edges only. Walk rows are complete: nothing is cut to a page size (the viewer pages them); ranked answers have a default [row count](#walk-flags-and-row-counts).

| Need | Read |
|---|---|
| `search`, `context`, `about`, `files`, `links` | [Find code](query-search.md) |
| `impact`, `deps`, `dependents`, `path`, `region`, `order`, `trace`, `cycle`, `community`, `callers`, `callees`, `critical`, `top` | [Walks and rankings](query-walks.md) |
| `reach`, `why`, `hubs`, `bundle`, `hot`, a minified frame | [What loads and what it costs](query-loads.md) |
| Several questions in one load; asking from the viewer, the library, MCP or the channel | [Where to ask](query-surfaces.md) |
| Pointing at nodes in a reply · the wiki through `reply wiki` or the channel | [Chat](chat.md) · [wiki operations](wiki.md#operations) |

## Walk flags and row counts

Every import records its kind (`static-import`, `type-import`, `dynamic-import`, `named-reexport`, `type-named-reexport`, `commonjs-require`, `rust-use`, …). Walks over every edge tag a file reached through `import type` with `load: type` (it never loads) and one reached through `import()` with `load: deferred`, and the summary says how many and names `--runtime`. Path hops carry `load`, and their `evidence` lists the hop's import `kinds`.

| Flag | Walks |
|---|---|
| `--runtime` | Imports that run: drops type-only imports (`import type`, `export type`, an import TypeScript elides). In JS/TS and Python a call alone loads nothing, so only import pairs; where a language binds at compile time (Go files of one package, C# and Scala namespaces, Java, Rust, C) a resolved call is a dependency too, so its call pairs stay as `compile`. `top`, `cycle` and `critical points` measure again over these edges |
| `--eager` | Imports that run while the importer loads: also drops `import()` and Python imports inside functions. What loads at startup |
| `--count` | The count only (`count`), rows left out |
| `--group dir\|dir:N\|chunk\|package\|depth` | One row per group with its `count` and `bytes`: `dependents util.ts depth all --group dir:2` |

An import whose load the graph cannot establish (a parser fallback, a value import used only as a type, an import between chunks from an engine that did not record its kind) counts as loading; `reach` lists each as `unsettled`.

Ranked answers (`search`, `top`, `hubs`, `critical`, `hot`) show the best 20 rows unless given a count or `all` (`--all` for `search`); the summary names the form that lists every row.

## Arguments and results

A file is a root-relative path, an absolute path under the root, `path:line`, a unique suffix (`pipeline.ts`) or a node id. An ambiguous or unknown argument is an error with every candidate as a re-runnable query (`impact b/util.ts`, `impact c/util.ts`); nothing is guessed.

Queries may be written naturally: filler words (`callers of x`, `path from a to b`), op typos and synonyms (`serach`, `cycles`, `dependencies`, `who calls x`), metric nicknames (`top pr`, `top in`), a whole query in quotes, and a misspelled file or symbol with one near match are each read as one exact query. The result's `readAs` gives that query and each change, and the text output starts with `read as:`. When a misspelling or a typo is near several names, nothing is answered and the candidates come back instead.

Results include `scope` with the export root, snapshot label and known coverage status. A null coverage status means the export did not record it. `nodes` are the ids to highlight, `edges` the `[u, v]` pairs that carry the answer, each row `{ id, label, chunk, loc: { path, line }, ... }` plus its op's fields (`depth`, `weight`, `through`, `nextFile`, `order`, `rank`, `value`, `symbol`, `via`, `evidence`).
