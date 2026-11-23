# Saved graph format

A build writes one **result directory** (default `.octocode/octocode-graph/<date>-<name>-<id>/`). Every path inside
it is relative to the graph's root, stored once as `data.json` `section`. Nothing in it is required to stay: delete
the directory to drop the graph.

## Layout

Measured on `packages/octocode-native` (3,520 files, 19 chunks, 70,031 calls) on 2026-11-23:

| Path | Format | Size | Written | Read by |
|---|---|--:|---|---|
| `data.json` | One JSON document | 34 MB | Atomically (temp file, then rename) at the end of a build or refresh | `query`, `diff`, the viewer's answers, `html`, the library's `openGraph` |
| `previous.json` | Same as `data.json` | 0 until a refresh changes the graph | A refresh hard-links the replaced `data.json` here | `diff <dir>`, `diff_graph({result})` |
| `brief.txt` | Text | 4 KB | With `data.json` | Agents first: counts, top PageRank, betweenness, cycles, packages, checks |
| `events.jsonl` | JSON Lines, append-only | 12 MB | Every event as it happens; a refresh appends. A `rows` event keeps its page and count, not its rows, which its chunk result holds | `serve --replay` and `html` (rows rebuilt from the chunk results), `refresh` (the run's root and listing) |
| `chunks/<id>/data.json.gz` | gzip JSON | 4.9 MB in all | Atomically when the chunk is built | `refresh` (reuses unchanged chunks), stitching |
| `chunks/<id>/evidence/*.jsonl.gz` | gzip JSON Lines, one native page per line | 7.5 MB in all | While the chunk is built | People auditing a row: `gzip -dc … \| rg <id>` |
| `links.json` | JSON | — | `octocode-graph link … add` | Agents' links with their quotes and hashes; kept across rebuilds and checked by each |
| `wiki/` | Markdown | 0.9 MB | After metrics | `wiki read`, the viewer |
| `server.json` | JSON, mode 0600 | — | While a viewer serves the run | `reply`, `refresh` (they go through that viewer) |
| `model.json` | JSON, optional, written by you | — | — | `render`, the brief's layer checks |

`stitched.json` exists only while a build runs, so queries can answer before LSP finishes; `data.json` replaces it.

## `data.json`

| Field | Holds |
|---|---|
| `section` | The absolute root; a path's absolute form is `section/path` |
| `files[]` | `id` (root-relative path), `label`, `language`, `roles`, `chunk`, `fanIn`, `fanOut`, `check` (on-disk status), `metrics` (PageRank, betweenness, in/out weight, depth, core, community, cycle); workspace package nodes add `kind: "package"` and `manifest` |
| `symbols[]` | Functions and other callables in the call graph: `id` (`file#name`), `file`, `name`, `kind`, `line`, `endLine`, `exported`, `calls` |
| `imports[]`, `calls[]` | `source`, `target`, `line`, `lines`, `count`, `confidence`, `resolution`, `test`, `crossChunk` (stitched between chunks); calls add `via` (graph, lsp or both) |
| `fileEdges[]` | The weighted file graph: `weight` = import + evidence-weighted call sites, with `imports`, `calls`, `test` and `strong` |
| `metrics` | `cycles`, `communities`, `packages` (Ca, Ce, instability), `stats`, `weightRule` |
| `coverage`, `verification`, `callStats` | What the build saw and could not see: partial chunks, unbuilt chunks, files checked, LSP probes, answers that omitted locations outside the read roots (`withheld`) and unanswered calls |
| `links[]`, `unresolvedLinks[]` | Docs, manifests and agents → files and symbols: `source`, `line`, `target`, `rule`, `relation`, `check`; what named no single target with `reason` and `candidates`. Never in the code metrics |

Nodes and edges store their own lines; query rows add `loc: {path, line}` when they are answered.

## Why this format

Measured on the same workspace, before its cross-crate calls were bound (63,560 calls):

| Option | Size | Load | Verdict |
|---|--:|--:|---|
| **JSON, each fact once** (chosen) | 32 MB | 106 ms | One parse loads the whole graph; `jq`, the browser viewer and the HTML export read it directly |
| JSON with a stored location per node and edge (before) | 55 MB | 280 ms | Each absolute path repeated the root: 37% of the file |
| gzip of `data.json` | 2.7 MB | 150 ms | 12× smaller, 40% slower to load, and every reader needs a decode step. Used for the cold internal files instead |
| `v8.serialize` | 32 MB | 112 ms | No gain over JSON; Node only, unreadable to `jq` and the browser |
| SQLite (`node:sqlite`) | 24 MB for symbols and calls alone | 134 ms to read every row | Point lookups take 0.2 ms, but PageRank, cycles and paths need the whole graph in memory, and the browser cannot open it |

- **Folder, not one file:** a refresh reuses unchanged chunks, appends to the log and keeps the previous graph by
  hard link; one file would be rewritten whole. For one portable file, export with `octocode-graph html`.
- **Plain where people and agents read it, gzip where only the tool does:** `data.json`, `brief.txt` and
  `events.jsonl` stay plain; chunk results and raw evidence are gzipped (12 MB instead of 73 MB), and the event log holds the graph rows by reference to the chunk results (12 MB instead of 51 MB).
- **Crash-safe:** `data.json` and chunk results are written to a temporary file and renamed, so a reader never sees
  half a graph.

## Store, diff, check, search

| Task | Do | Not |
|---|---|---|
| Keep or share a graph | `tar czf graph.tgz <dir>` (60 MB → 16 MB here), or `octocode-graph html <dir> graph.html` for one offline page | Commit result directories: they are build output, under `.octocode/` |
| Update after edits | `octocode-graph refresh <dir>`: only chunks with changed files rebuild (2.1 s when nothing changed on this graph) | Rebuild from scratch |
| Diff | `octocode-graph diff <dir>` after a refresh, or `diff <before> <after>`: new cycles, package links against stability, reach and ranking changes, then every dependency added or removed | `diff`/`git diff` on `data.json`: one line of ids and metrics, no meaning |
| Check | `brief.txt`, then `query <dir> "about <file>"`; coverage gaps: `jq '.coverage, .callStats.lsp' data.json` | Read the brief as proof: cite the rows' `path:line` |
| Search | `query <dir> "search <name>"` (ranked: exact, prefix, initials, substring, misspelling) | `rg` over `data.json`: a match prints the whole 34 MB line |
| Read the source | The answer's `read:` line (`localFetch` with the cited range), and `confirm:` (`lspSearch` callers) before calling a list complete | — |
