# Octocode Graph

Build, query and replay repository graphs with the bundled [graph skill](skills/octocode-graph/SKILL.md). The package uses the native engine supplied by its `octocode` dependency for graph construction, queries and language-server confirmation.

Install on Node 24.15 or later in the 24.x series. From the project to inspect, run:

```sh
npm install --save-dev @octocodeai/octocode-graph
npx octocode-graph build . --out .octocode/first-run
npx octocode-graph serve --replay .octocode/first-run
```

Without installing, every command runs as `npx -y @octocodeai/octocode-graph <command>`; the CLI's hints name `octocode-graph` only when that command on your PATH runs this package.

The default `--lsp smart` checks outgoing calls from declarations in hub files and callers linked across chunks, and every function holding a receiver call the syntax graph could not bind to a method declared here. Choose `--lsp all` for every indexed function, method and constructor with a declaration line, or `--lsp off` for a static export. The saved run records unanswered declarations and their reasons.

The package exports two native tools, `graphBuild` to create a snapshot and `graphQuery` to query it. Run them through the graph CLI from your project directory.

```sh
npx octocode-graph graphBuild '{"queries":[{"path":"."}]}' --json
npx octocode-graph graphQuery '{"queries":[{"path":".","operation":"stats"}]}' --json
npx octocode-graph schema graphQuery --view variants
npx octocode-graph graphQuery '{"queries":[{"path":".","operation":"cycles"}]}' --all-pages
```

`--all-pages` reads a `graphQuery` call to its end (every page, `rowPart` fragment, evidence list and cycle member list, each checked against its stated count) and prints one JSON reply without `next`, each row once.

To connect an MCP client, add a stdio server entry.

```json
{
  "mcpServers": {
    "octocode-graph": {
      "command": "npx",
      "args": ["-y", "@octocodeai/octocode-graph", "mcp"]
    }
  }
}
```

This one server carries everything: `build_graph`, `query_graph`, `refresh_graph`, `diff_graph`, `check_graph` and `link_graph` for saved graphs, and the engine's own `graphBuild` and `graphQuery` passed through unchanged (cursors and errors included). Local access and tool filters still apply. `octocode-graph channel` is the same server plus the live viewer, its chat and its wiki, as a Claude Code channel.

In JavaScript, the saved-graph API and the native tools come from `@octocodeai/octocode-graph`:

```js
import { buildGraph, openFreshGraph, openGraph, refreshGraph } from '@octocodeai/octocode-graph';

const { out } = await buildGraph('.', { lsp: 'smart' });
const graph = openGraph(out);                    // or a project directory: its newest saved graph
graph.query('about src/main.ts');               // the same result objects as the CLI's --json
await refreshGraph(out);                        // after edits; keeps previous.json
(await openFreshGraph('.')).query('search main'); // refreshes edited chunks first, then answers
openGraph(out).diffFrom(`${out}/previous.json`); // review findings and every changed dependency
```

`graphBuild`, `graphQuery` and `startMcp` expose the native engine directly: both tool functions accept the native input envelope and return the native response, including cursors. Import `runGraphCli` from `@octocodeai/octocode-graph/cli` to delegate a CLI argument array and receive its exit code. `@octocodeai/octocode-graph/skill` resolves to the bundled skill manifest.

Pass a source file for a single-file graph or a directory for cross-file relationships. For example, `octocode-graph build src/main.ts --out ./main-graph` exports one file and `octocode-graph build src --out ./src-graph` exports the directory.

Each new build needs a fresh result directory. Omit `--out` to allocate one automatically. Querying a saved result works offline. What the directory holds, and how to keep, diff, check and search it: [saved graph format](docs/graph-output.md).

| Command | Result |
| --- | --- |
| `octocode-graph build <directory> [--out <result-dir>]` | Build a stitched graph, evidence and briefs, and register the result for its root. A directory inside a workspace names the workspace packages it left as externals and the build of the workspace root that stitches them; a directory inside one package names its imports into the rest of that package and the build of the package root. |
| `octocode-graph query <result-dir> "deps src/main.ts" [--refresh]` | Query a saved graph or a live checkpoint. A project directory selects its newest saved graph under `.octocode/octocode-graph`, else the newest one registered for it (built with `--out` elsewhere). `--refresh` refreshes edited chunks first. |
| `octocode-graph refresh <result-dir>` | After edits, rebuild only the chunks holding a change, ask the language server again only about code an edit could change, and measure the whole graph again. The replaced graph stays as `previous.json`. |
| `octocode-graph diff <result-dir>` or `diff <before> <after>` | Review a change by structure: new or grown cycles, new package dependencies (flagged against stability), files with more dependents, new hotspots, then every dependency added or removed. |
| `octocode-graph html <result-dir> <new.html>` | Save a self-contained interactive HTML file with the full replay and queryable graph. Open it directly in a modern browser with WebGL. Saved queries run in a browser worker. |
| `octocode-graph serve <directory>` | Open an interactive graph. |
| `octocode-graph serve --replay <result-dir>` | Reopen a saved run, including runs created by the former viewer skill. |
| `octocode-graph events <result-dir>` | Summarize the run's `events.jsonl`: tool calls, cursor pages, errors, rows, bytes and time per tool; wall time and phases per build and refresh. |
| `octocode-graph check [<result-dir\|project-dir>] [--rules <file>] [--format text\|json\|sarif]` | Check dependency rules (forbidden, allowed, required, reachable, circular) for CI, with `--baseline`, `--write-baseline` and `--since <git-rev>`. Rules default to `.octocode/graph-rules.json`: [rules](skills/octocode-graph/references/rules.md). |
| `octocode-graph link <result-dir> add\|remove\|review\|list` | Store an agent's link between files, kept only while each quote sits at its cited lines: [links](skills/octocode-graph/references/links.md). |
| `octocode-graph wiki build\|read\|note\|promote\|lint <result-dir> …` | Build, read, annotate or lint the code wiki of a saved graph: [wiki](skills/octocode-graph/references/wiki.md). |
| `octocode-graph locate <bundle.js:line:col>… [--graph <result-dir>]` | Map bundled or minified positions to source lines through source maps, and with `--graph` to the graph symbol holding them. |
| `octocode-graph cache stats` or `cache prune --max-bytes N` | Show or prune the shared cache of chunk results, LSP answers and derived data. |
| `octocode-graph preflight [--json]` | Report what can run here (engine, language servers) and how to enable the rest. |
| `octocode-graph render --data <data.json>` | Write `brief.txt` beside a `data.json`. |
| `octocode-graph reply <result-dir> "text"` | Answer a viewer chat, query or wiki request through the viewer serving the run. |

See the [viewer commands](skills/octocode-graph/references/commands.md) for scope, LSP options and progress.

## Analyze a saved graph

```sh
npx octocode-graph query . 'search parseArgs'             # newest saved graph of this directory
npx octocode-graph query . 'about src/main.ts'             # metrics, community, direct deps and dependents
npx octocode-graph query . 'critical functions'            # functions much of the code calls into
npx octocode-graph query . 'critical path src/lib.ts --up' # the longest chain a change can travel up
npx octocode-graph query .octocode/first-run 'order src/main.ts --imports'
npx octocode-graph query .octocode/first-run 'trace src/main.ts DEFINES CALLS+' --json
```

`search` ranks files and symbols by how their name matches (exact ignoring case and separators and prefix always first, then word initials, substring, every word, path) fused with BM25F over each declaration's words, path, doc comment, the names it calls and its signature (from `search-index.json`, built beside `data.json` on the first search), then by call sites and PageRank; `--kind`, `--lang`, `--role`, `--path`, `--exported`, `--param` and `--returns` filter, `--provider` reranks the top 20 through the configured `clasify` provider. `context <files|symbols> --tokens N` maps what surrounds them by personalized PageRank under a token budget. `about` answers everything adjacent to one file or symbol. A project directory selects its newest saved graph, including one built with `--out` elsewhere (the per-user graph registry in `$OCTOCODE_HOME/graph-registry`, default `~/.octocode`); stderr names the run, and every answer warns of graph files edited since it was written. Add `--refresh` to rebuild the chunks holding them before answering.

The first query lists prerequisite groups before their consumers, including the source evidence for each crossing. The second follows declarations and one or more calls, returning matching endpoints and the witnesses on accepting walks. Both commands also work in the viewer. [Saved query syntax](skills/octocode-graph/references/query.md) · [`order` and `trace`](skills/octocode-graph/references/query-walks.md)

To attach a SCIP JSON index, per-test LLVM coverage, LLVM control-flow and dominance graphs, or OTLP JSON spans, write the manifest with `octocode-graph enrich --manifest-from data.json manifest.json --revision <rev> --provider <format> <file>` (it records every digest), then run `octocode-graph enrich data.json manifest.json enriched.json`. Query the new file directly. [Provider manifest](skills/octocode-graph/references/provider-manifest.md) · [evidence](skills/octocode-graph/references/providers.md)

## Develop and pack

Everything in `src/` is TypeScript. Node code (the CLI, the MCP launcher, `src/runtime` and its command scripts in `src/runtime/commands`) is checked by `tsconfig.json`; the viewer (`src/assets`) and the offline replay and query worker (`src/browser`) by `tsconfig.browser.json`. Sources import each other by their `.ts` names, so tests run them directly. Each command script in `src/runtime/commands` only calls its module's `main`: a library module never runs itself, because the bundler moves a module that other entries import into a shared chunk, where a check of `import.meta.url` against the process entry no longer matches. The build uses esbuild to bundle the Node entry points and command scripts as one graph with shared chunks, bundles the viewer and the offline replay for the browser, and emits declarations for TypeScript consumers. Tool input and output types come from the same schema as the native engine.

```sh
yarn workspace @octocodeai/octocode-graph verify
cd packages/octocode-graph
npm pack
```

Verification runs strict TypeScript checks, formatting checks, Oxlint, the skill checker and package tests. Packing runs the build. The tarball includes the CLI, ESM entry points, declarations and portable skill. The viewer loads its WebGL renderer from the local server, including during replay. The native executable resolves through the installed `octocode` dependency.


The graph skill is a Markdown cookbook. The package CLI, the saved-graph MCP server, the viewer and the exported HTML page use the same search and traversal implementation under `src/runtime`.

```sh
octocode-graph query .octocode/first-run 'files src/main.ts:18 src/lib.ts:42' --json
octocode-graph query .octocode/first-run 'impact src/main.ts src/lib.ts depth 2'
```

Over MCP, `octocode-graph mcp` answers `query_graph` with a saved result directory and the same query string, without a browser.

### Links beside the code

A build also reads every Markdown file and manifest under the root. Links, backticked paths, `path:line` citations and placed symbol names in docs, and the files that `package.json`, `Cargo.toml` and `tsconfig` fields name, become links. Each link cites its line and never counts in the code metrics. A reference that names no single target is kept with its reason and candidates. A citation whose code changed under an unchanged doc line turns `stale` on refresh.

```sh
octocode-graph query <out> 'links --unresolved'      # what static rules could not settle
octocode-graph link <out> add '{"relation":"documents","source":"README.md","target":"src/cli.ts#parseArgs","claim":"…","anchors":[…],"proposedBy":"agent"}'
```

An agent's link is stored only when each quote is at its cited lines and occurs once in its file. Every build checks it again: a moved quote moves the anchor, a changed one makes the link stale. [Links](skills/octocode-graph/references/links.md)
