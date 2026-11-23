# Octocode Graph

Build, query and replay repository graphs with the bundled [graph skill](skills/octocode-graph/SKILL.md). The package uses the native engine supplied by its `octocode` dependency for graph construction, queries and language-server confirmation.

Install on Node 24.15 or later in the 24.x series. From the project to inspect, run:

```sh
npm install --save-dev @octocodeai/octocode-graph
npx octocode-graph-build . --out .octocode/first-run
npx octocode-graph-serve --replay .octocode/first-run
```

The default `--lsp smart` checks outgoing calls from declarations in hub files and callers linked across chunks. Choose `--lsp all` for every indexed function, method and constructor with a declaration line, or `--lsp off` for a static export. The saved run records unanswered declarations and their reasons.

The package exports two native tools, `graphBuild` to create a snapshot and `graphQuery` to query it. Run them through the graph CLI from your project directory.

```sh
npx octocode-graph graphBuild '{"queries":[{"path":"."}]}' --json
npx octocode-graph graphQuery '{"queries":[{"path":".","operation":"stats"}]}' --json
npx octocode-graph schema graphQuery --view variants
```

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

The graph server opts into native graph tools. Local access and tool filters still apply. In JavaScript, import `graphBuild`, `graphQuery`, or `startMcp` from `@octocodeai/octocode-graph`. Import `runGraphCli` from `@octocodeai/octocode-graph/cli` to delegate a CLI argument array and receive its exit code. Both tool functions accept the native input envelope and return the native response, including cursors. `@octocodeai/octocode-graph/skill` resolves to the bundled skill manifest.

Pass a source file for a single-file graph or a directory for cross-file relationships. For example, `octocode-graph-build src/main.ts --out ./main-graph` exports one file and `octocode-graph-build src --out ./src-graph` exports the directory.

Each new build needs a fresh result directory. Omit `--out` to allocate one automatically. Querying a saved result works offline.

| Command | Result |
| --- | --- |
| `octocode-graph-build <directory> [--out <result-dir>]` | Build a stitched graph, evidence and briefs. |
| `octocode-graph-query <result-dir> "deps src/main.ts"` | Query a saved graph or a live checkpoint. |
| `octocode-graph html <result-dir> <new.html>` | Save a self-contained interactive HTML file with the full replay and queryable graph. Open it directly in a modern browser with WebGL. Saved queries run in a browser worker. |
| `octocode-graph-serve <directory>` | Open an interactive graph. |
| `octocode-graph-serve --replay <result-dir>` | Reopen a saved run, including runs created by the former viewer skill. |

See the [viewer commands](skills/octocode-graph/references/commands.md) for scope, LSP options and progress.

## Analyze a saved graph

```sh
npx octocode-graph-query .octocode/first-run 'order src/main.ts --imports'
npx octocode-graph-query .octocode/first-run 'trace src/main.ts DEFINES CALLS+' --json
```

The first query lists prerequisite groups before their consumers, including the source evidence for each crossing. The second follows declarations and one or more calls, returning matching endpoints and the witnesses on accepting walks. Both commands also work in the viewer. [Saved query syntax](skills/octocode-graph/references/query.md)

To attach a SCIP JSON index, per-test LLVM coverage, LLVM control-flow and dominance graphs, or OTLP JSON spans, run `octocode-graph enrich data.json manifest.json enriched.json`. Query the new file directly. [Provider manifest and evidence](skills/octocode-graph/references/providers.md)

## Develop and pack

The public Node entry points, algorithms and provider adapters live in `src/`. The portable skill contains the viewer and extraction scripts. The build uses esbuild to bundle Node ESM with shared chunks, emits declarations for TypeScript consumers, and generates standalone copies of the algorithms used by a copied skill. Tool input and output types come from the same schema as the native engine.

```sh
yarn workspace @octocodeai/octocode-graph verify
cd packages/octocode-graph
npm pack
```

Verification runs strict TypeScript checks, formatting checks, Oxlint, the skill checker and package tests. Packing runs the build. The tarball includes the CLI, ESM entry points, declarations and portable skill. The viewer loads its WebGL renderer from the local server, including during replay. The native executable resolves through the installed `octocode` dependency.


The graph skill is a Markdown cookbook. The package CLI and saved-graph MCP server use the same search and traversal implementation under `src/runtime`.

```sh
octocode-graph query .octocode/first-run 'files src/main.ts:18 src/lib.ts:42' --json
octocode-graph query .octocode/first-run 'impact src/main.ts src/lib.ts depth 2'
```

Start `octocode-graph channel` as a stdio MCP server and call `query_graph` with a saved result directory and the same query string. This works without a browser. Use `octocode-graph mcp` for the native `graphBuild` and `graphQuery` snapshot tools.
